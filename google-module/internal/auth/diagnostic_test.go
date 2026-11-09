package auth

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/router-for-me/CLIProxyAPI/v7/sdk/pluginapi"
)

func TestSetupDiagnosticExcludesAllRawLoginMaterial(t *testing.T) {
	base := &accountSetupClient{load: `{"allowedTiers":[{"id":"free-tier","isDefault":true,"description":"PRIVATE-DESCRIPTION"},{"id":"PRIVATE-TIER"}],"ineligibleTiers":[{"reasonCode":"RESTRICTED_NETWORK","reasonMessage":"PRIVATE-REASON"},{"reasonCode":"PRIVATE-REASON-CODE"}],"email":"PRIVATE-EMAIL","access_token":"PRIVATE-TOKEN","cloudaicompanionProject":"PRIVATE-PROJECT"}`}
	diagnostic := &setupDiagnosticClient{client: base}
	_, err := diagnostic.Do(context.Background(), pluginapi.HTTPRequest{URL: codeAssistBaseURL + "/" + codeAssistVersion + ":loadCodeAssist", Body: []byte(`{}`)})
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	diagnostic.save(dir)
	data, err := os.ReadFile(filepath.Join(dir, "last-gemini-login.diagnostic"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(data), "PRIVATE") {
		t.Fatalf("diagnostic leaked input: %s", data)
	}
	var result map[string]any
	if json.Unmarshal(data, &result) != nil || !strings.Contains(string(data), "RESTRICTED_NETWORK") || !strings.Contains(string(data), "free-tier") {
		t.Fatalf("missing safe diagnosis: %s", data)
	}
}

func TestIneligibleReasonSurvivesMissingOnboardingProject(t *testing.T) {
	c := &accountSetupClient{load: `{"ineligibleTiers":[{"reasonCode":"UNSUPPORTED_LOCATION","reasonMessage":"PRIVATE-MESSAGE"}]}`, onboard: `{"done":true,"response":{}}`}
	s, err := NewProvider().finalizeCode(context.Background(), c, "fixture-code", map[string]any{"redirect_uri": "http://127.0.0.1/fixture"})
	if err == nil || s != nil || !strings.Contains(err.Error(), "UNSUPPORTED_LOCATION") || strings.Contains(err.Error(), "PRIVATE") {
		t.Fatalf("lost safe eligibility reason: %v", err)
	}
}
