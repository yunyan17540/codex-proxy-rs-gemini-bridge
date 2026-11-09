package auth

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/router-for-me/CLIProxyAPI/v7/sdk/pluginapi"
)

// 仅保留预定义枚举和字段形状，不能记录请求、令牌、项目 ID、邮箱或返回原文。
type setupDiagnosticClient struct {
	client pluginapi.HostHTTPClient
	steps  []map[string]any
}

func safeSetupReason(value any) string {
	reason, _ := value.(string)
	switch reason {
	case "DASHER_USER", "INELIGIBLE_ACCOUNT", "NON_USER_ACCOUNT", "RESTRICTED_AGE", "RESTRICTED_NETWORK", "UNKNOWN_LOCATION", "UNSUPPORTED_LOCATION", "VALIDATION_REQUIRED", "SUBSCRIPTION_REQUIRED", "SERVICE_DISABLED", "TOS_VIOLATION", "PERMISSION_DENIED":
		return reason
	default:
		return "UNKNOWN"
	}
}

func setupReasons(response map[string]any) []string {
	var reasons []string
	if tiers, ok := response["ineligibleTiers"].([]any); ok {
		for _, value := range tiers {
			if tier, ok := value.(map[string]any); ok && len(reasons) < 16 {
				reasons = append(reasons, safeSetupReason(tier["reasonCode"]))
			}
		}
	}
	return reasons
}

func safeSetupTier(value any) string {
	id, _ := value.(string)
	switch id {
	case "free-tier", "legacy-tier", "standard-tier", "g1-pro-tier", "g1-ultra-tier":
		return id
	default:
		return "other"
	}
}

func setupShape(value any) string {
	switch value.(type) {
	case nil:
		return "missing"
	case map[string]any:
		return "object"
	case string:
		return "string"
	default:
		return "other"
	}
}

func (c *setupDiagnosticClient) Do(ctx context.Context, req pluginapi.HTTPRequest) (pluginapi.HTTPResponse, error) {
	response, err := c.client.Do(ctx, req)
	if !strings.HasPrefix(req.URL, codeAssistBaseURL+"/") || len(c.steps) >= 16 {
		return response, err
	}
	phase := "operation"
	if strings.HasSuffix(req.URL, ":loadCodeAssist") {
		phase = "loadCodeAssist"
	} else if strings.HasSuffix(req.URL, ":onboardUser") {
		phase = "onboardUser"
	}
	step := map[string]any{"phase": phase, "httpStatus": response.StatusCode, "transportFailure": err != nil}
	var body map[string]any
	if len(response.Body) <= 2*1024*1024 && json.Unmarshal(response.Body, &body) == nil {
		step["currentTierShape"] = setupShape(body["currentTier"])
		step["projectShape"] = setupShape(body["cloudaicompanionProject"])
		step["projectPresent"] = projectIDFromCodeAssistValue(body["cloudaicompanionProject"]) != ""
		if tier, ok := body["currentTier"].(map[string]any); ok {
			step["currentTier"] = safeSetupTier(tier["id"])
		}
		var tiers []map[string]any
		if allowed, ok := body["allowedTiers"].([]any); ok {
			for _, value := range allowed {
				if tier, ok := value.(map[string]any); ok && len(tiers) < 16 {
					isDefault, _ := tier["isDefault"].(bool)
					userProject, _ := tier["userDefinedCloudaicompanionProject"].(bool)
					tiers = append(tiers, map[string]any{"id": safeSetupTier(tier["id"]), "isDefault": isDefault, "userProjectRequired": userProject})
				}
			}
		}
		step["allowedTiers"] = tiers
		step["ineligibleReasons"] = setupReasons(body)
		step["responseShape"] = setupShape(body["response"])
		step["errorPresent"] = body["error"] != nil
		if done, ok := body["done"].(bool); ok {
			step["done"] = done
		}
		if result, ok := body["response"].(map[string]any); ok {
			step["responseProjectShape"] = setupShape(result["cloudaicompanionProject"])
			step["responseProjectPresent"] = projectIDFromCodeAssistValue(result["cloudaicompanionProject"]) != ""
		}
	}
	c.steps = append(c.steps, step)
	return response, err
}

func (c *setupDiagnosticClient) DoStream(ctx context.Context, req pluginapi.HTTPRequest) (pluginapi.HTTPStreamResponse, error) {
	return c.client.DoStream(ctx, req)
}

func (c *setupDiagnosticClient) save(authDir string) {
	if strings.TrimSpace(authDir) == "" {
		return
	}
	data, err := json.Marshal(map[string]any{"schema": 1, "time": time.Now().UTC().Format(time.RFC3339), "steps": c.steps})
	if err == nil {
		// 此目录已由外层限制为当前用户和 SYSTEM；扩展名避免被当作账号导入。
		_ = os.WriteFile(filepath.Join(authDir, "last-gemini-login.diagnostic"), data, 0600)
	}
}
