//! Windows Job Object 持有本实例引擎，插件异常结束时由内核回收子进程

use std::{
    io,
    mem::size_of,
    os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    path::Path,
    process::{Child, Command, Stdio},
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::{
    System::SystemInformation::GetWindowsDirectoryW,
    UI::{
        Shell::{CSIDL_PROFILE, SHGetFolderPathW, ShellExecuteW},
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

/// 宿主清空插件环境。WinSock 和 Go 登录流程需要 OS 路径，不继承任何网关凭据。
/// 此函数仅在单线程 main 的第一步调用，先于 Tokio 或其他后台线程。
pub fn initialize_windows_environment() -> io::Result<()> {
    let mut buffer = vec![0u16; 32768];
    // SAFETY: 缓冲区可写，长度正确。
    let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(io::Error::last_os_error());
    }
    let windows = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]));
    let system = windows.join("System32");
    let mut profile = [0u16; 260];
    // SAFETY: profile 有 MAX_PATH 长度，句柄为空表示当前用户。
    if unsafe {
        SHGetFolderPathW(
            std::ptr::null_mut(),
            CSIDL_PROFILE as i32,
            std::ptr::null_mut(),
            0,
            profile.as_mut_ptr(),
        )
    } < 0
    {
        return Err(io::Error::other("无法定位当前 Windows 用户目录"));
    }
    let length = profile
        .iter()
        .position(|c| *c == 0)
        .ok_or_else(|| io::Error::other("Windows 用户目录无效"))?;
    let profile = std::path::PathBuf::from(std::ffi::OsString::from_wide(&profile[..length]));
    let paths = [
        ("SystemRoot", windows.clone()),
        ("WINDIR", windows.clone()),
        ("COMSPEC", system.join("cmd.exe")),
        ("USERPROFILE", profile.clone()),
        ("LOCALAPPDATA", profile.join("AppData/Local")),
        ("APPDATA", profile.join("AppData/Roaming")),
        ("TEMP", profile.join("AppData/Local/Temp")),
        ("TMP", profile.join("AppData/Local/Temp")),
    ];
    for (key, value) in paths {
        if std::env::var_os(key).is_none() {
            // SAFETY: 只由 main 在启动任何其他线程之前设置 OS 路径。
            unsafe { std::env::set_var(key, value) };
        }
    }
    if std::env::var_os("PATH").is_none() {
        let path = std::env::join_paths([system.as_path(), windows.as_path()])
            .map_err(io::Error::other)?;
        // SAFETY: 同上，此时尚无其他线程。
        unsafe { std::env::set_var("PATH", path) };
    }
    Ok(())
}

pub fn open_google_login(url: &str) -> io::Result<()> {
    // 仅由显式按钮调用；地址已在创建登录会话时限定为 Google 官方 HTTPS URL。
    let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    // SAFETY: UTF-16 字符串均以 NUL 结尾，在调用期间有效；不传递可执行命令。
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err(io::Error::other("默认浏览器未打开"))
    }
}

pub struct EngineProcess {
    child: Child,
    _job: OwnedHandle,
}

impl EngineProcess {
    pub fn spawn(executable: &Path, config: &Path, directory: &Path) -> io::Result<Self> {
        // SAFETY: 使用未命名且不可继承的 Job，成功后立即交给 OwnedHandle 唯一持有
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: handle 非空，所有权尚未被其他对象接管
        let job = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: 指针和结构长度匹配 Win32 合同，Job 在调用期间有效
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut child = Command::new(executable)
            .args(["-config"])
            .arg(config)
            .arg("-local-model")
            .current_dir(directory)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // SAFETY: Job 和子进程均由当前对象持有，分配失败时立即结束自己刚启动的子进程
        if unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } == 0 {
            let error = io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self { child, _job: job })
    }

    pub fn running(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_none())
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        // 仅回收持有句柄对应的本实例子进程，不按名称结束其他网关
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn protect_directory(directory: &Path) -> io::Result<()> {
    let identity = Command::new("whoami.exe")
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !identity.status.success() {
        return Err(io::Error::other("无法读取 Windows 用户身份"));
    }
    // SID 为 ASCII，不依赖 Windows 用户名的语言或当前控制台代码页。
    let text = String::from_utf8_lossy(&identity.stdout);
    let sid = text
        .trim()
        .rsplit_once(",\"")
        .map(|(_, tail)| tail.trim_end_matches('"'))
        .filter(|sid| {
            sid.starts_with("S-1-")
                && sid.len() < 256
                && sid
                    .chars()
                    .all(|ch| ch.is_ascii_digit() || ch == 'S' || ch == '-')
        })
        .ok_or_else(|| io::Error::other("Windows 用户 SID 无效"))?;
    let grant = format!("*{sid}:(OI)(CI)F");
    let status = Command::new("icacls.exe")
        .arg(directory)
        .args(["/inheritance:r", "/grant:r", &grant, "*S-1-5-18:(OI)(CI)F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("无法设置账号目录访问控制"))
    }
}
