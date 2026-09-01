// Package main implements the Stackhouse Terraform provider.
//
// This provider enables platform teams to manage Stackhouse resources
// (databases, storage buckets, functions, auth configs, etc.) via
// Infrastructure-as-Code.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
	"github.com/hashicorp/terraform-plugin-sdk/v2/plugin"
)

func main() {
	plugin.Serve(&plugin.ServeOpts{
		ProviderFunc: Provider,
	})
}

// Provider returns the Stackhouse Terraform provider schema.
func Provider() *schema.Provider {
	return &schema.Provider{
		Schema: map[string]*schema.Schema{
			"api_url": {
				Type:        schema.TypeString,
				Required:    true,
				DefaultFunc: schema.EnvDefaultFunc("STACKHOUSE_API_URL", "http://localhost:8080"),
				Description: "The Stackhouse API endpoint URL.",
			},
			"api_key": {
				Type:        schema.TypeString,
				Required:    true,
				Sensitive:   true,
				DefaultFunc: schema.EnvDefaultFunc("STACKHOUSE_API_KEY", nil),
				Description: "API key for authenticating with Stackhouse.",
			},
			"project_id": {
				Type:        schema.TypeString,
				Optional:    true,
				DefaultFunc: schema.EnvDefaultFunc("STACKHOUSE_PROJECT_ID", ""),
				Description: "Default project/tenant ID.",
			},
		},
		ResourcesMap: map[string]*schema.Resource{
			"stackhouse_database":        resourceDatabase(),
			"stackhouse_storage_bucket":  resourceStorageBucket(),
			"stackhouse_function":        resourceFunction(),
			"stackhouse_auth_provider":   resourceAuthProvider(),
			"stackhouse_rls_policy":      resourceRLSPolicy(),
			"stackhouse_vector_collection": resourceVectorCollection(),
			"stackhouse_webhook":         resourceWebhook(),
			"stackhouse_secret":          resourceSecret(),
		},
		DataSourcesMap: map[string]*schema.Resource{
			"stackhouse_project": dataSourceProject(),
		},
		ConfigureContextFunc: providerConfigure,
	}
}

type apiClient struct {
	BaseURL   string
	APIKey    string
	ProjectID string
	HTTP      *http.Client
}

func providerConfigure(ctx context.Context, d *schema.ResourceData) (interface{}, diag.Diagnostics) {
	client := &apiClient{
		BaseURL:   d.Get("api_url").(string),
		APIKey:    d.Get("api_key").(string),
		ProjectID: d.Get("project_id").(string),
		HTTP:      &http.Client{},
	}
	return client, nil
}

func (c *apiClient) doRequest(method, path string, body interface{}) (map[string]interface{}, error) {
	var reqBody io.Reader
	if body != nil {
		data, _ := json.Marshal(body)
		reqBody = strings.NewReader(string(data))
	}

	req, err := http.NewRequest(method, fmt.Sprintf("%s%s", c.BaseURL, path), reqBody)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", c.APIKey))
	req.Header.Set("Content-Type", "application/json")
	if c.ProjectID != "" {
		req.Header.Set("X-Project-ID", c.ProjectID)
	}

	resp, err := c.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 400 {
		respBody, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, string(respBody))
	}

	var result map[string]interface{}
	json.NewDecoder(resp.Body).Decode(&result)
	return result, nil
}

// === Resources ===

func resourceDatabase() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceDatabaseCreate,
		ReadContext:   resourceDatabaseRead,
		DeleteContext: resourceDatabaseDelete,
		Schema: map[string]*schema.Schema{
			"name": {Type: schema.TypeString, Required: true, ForceNew: true},
			"region": {Type: schema.TypeString, Optional: true, Default: "us-east-1"},
			"plan": {Type: schema.TypeString, Optional: true, Default: "starter"},
			"connection_string": {Type: schema.TypeString, Computed: true, Sensitive: true},
		},
	}
}

func resourceDatabaseCreate(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	client := m.(*apiClient)
	body := map[string]interface{}{
		"name":   d.Get("name").(string),
		"region": d.Get("region").(string),
		"plan":   d.Get("plan").(string),
	}
	resp, err := client.doRequest("POST", "/v1/admin/databases", body)
	if err != nil {
		return diag.FromErr(err)
	}
	d.SetId(fmt.Sprintf("%v", resp["id"]))
	if conn, ok := resp["connection_string"]; ok {
		d.Set("connection_string", conn)
	}
	return nil
}

func resourceDatabaseRead(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	client := m.(*apiClient)
	_, err := client.doRequest("GET", fmt.Sprintf("/v1/admin/databases/%s", d.Id()), nil)
	if err != nil {
		d.SetId("")
		return nil
	}
	return nil
}

func resourceDatabaseDelete(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	client := m.(*apiClient)
	_, err := client.doRequest("DELETE", fmt.Sprintf("/v1/admin/databases/%s", d.Id()), nil)
	if err != nil {
		return diag.FromErr(err)
	}
	d.SetId("")
	return nil
}

func resourceStorageBucket() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceBucketCreate,
		ReadContext:   resourceBucketRead,
		DeleteContext: resourceBucketDelete,
		Schema: map[string]*schema.Schema{
			"name":     {Type: schema.TypeString, Required: true, ForceNew: true},
			"public":   {Type: schema.TypeBool, Optional: true, Default: false},
			"max_size": {Type: schema.TypeInt, Optional: true, Default: 0},
		},
	}
}

func resourceBucketCreate(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	client := m.(*apiClient)
	body := map[string]interface{}{
		"name":   d.Get("name").(string),
		"public": d.Get("public").(bool),
	}
	resp, err := client.doRequest("POST", "/v1/storage/buckets", body)
	if err != nil {
		return diag.FromErr(err)
	}
	d.SetId(fmt.Sprintf("%v", resp["id"]))
	return nil
}

func resourceBucketRead(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	return nil
}

func resourceBucketDelete(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	client := m.(*apiClient)
	_, err := client.doRequest("DELETE", fmt.Sprintf("/v1/storage/buckets/%s", d.Id()), nil)
	if err != nil {
		return diag.FromErr(err)
	}
	d.SetId("")
	return nil
}

func resourceFunction() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceFunctionCreate,
		ReadContext:   resourceFunctionRead,
		DeleteContext: resourceFunctionDelete,
		Schema: map[string]*schema.Schema{
			"name":    {Type: schema.TypeString, Required: true},
			"runtime": {Type: schema.TypeString, Optional: true, Default: "wasm"},
			"source":  {Type: schema.TypeString, Required: true},
			"memory":  {Type: schema.TypeInt, Optional: true, Default: 128},
			"timeout": {Type: schema.TypeInt, Optional: true, Default: 30},
		},
	}
}

func resourceFunctionCreate(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	client := m.(*apiClient)
	body := map[string]interface{}{
		"name":    d.Get("name").(string),
		"runtime": d.Get("runtime").(string),
		"source":  d.Get("source").(string),
		"memory":  d.Get("memory").(int),
		"timeout": d.Get("timeout").(int),
	}
	resp, err := client.doRequest("POST", "/v1/functions", body)
	if err != nil {
		return diag.FromErr(err)
	}
	d.SetId(fmt.Sprintf("%v", resp["id"]))
	return nil
}

func resourceFunctionRead(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	return nil
}

func resourceFunctionDelete(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
	client := m.(*apiClient)
	_, err := client.doRequest("DELETE", fmt.Sprintf("/v1/functions/%s", d.Id()), nil)
	if err != nil {
		return diag.FromErr(err)
	}
	d.SetId("")
	return nil
}

func resourceAuthProvider() *schema.Resource {
	return &schema.Resource{
		CreateContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
			client := m.(*apiClient)
			body := map[string]interface{}{
				"provider":      d.Get("provider").(string),
				"client_id":     d.Get("client_id").(string),
				"client_secret": d.Get("client_secret").(string),
				"enabled":       d.Get("enabled").(bool),
			}
			resp, err := client.doRequest("POST", "/v1/auth/providers", body)
			if err != nil {
				return diag.FromErr(err)
			}
			d.SetId(fmt.Sprintf("%v", resp["id"]))
			return nil
		},
		ReadContext:   func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		DeleteContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		Schema: map[string]*schema.Schema{
			"provider":      {Type: schema.TypeString, Required: true},
			"client_id":     {Type: schema.TypeString, Required: true},
			"client_secret": {Type: schema.TypeString, Required: true, Sensitive: true},
			"enabled":       {Type: schema.TypeBool, Optional: true, Default: true},
		},
	}
}

func resourceRLSPolicy() *schema.Resource {
	return &schema.Resource{
		CreateContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
			client := m.(*apiClient)
			body := map[string]interface{}{
				"table":      d.Get("table").(string),
				"name":       d.Get("name").(string),
				"expression": d.Get("expression").(string),
				"operation":  d.Get("operation").(string),
			}
			resp, err := client.doRequest("POST", fmt.Sprintf("/v1/rls/%s/policies", d.Get("table").(string)), body)
			if err != nil {
				return diag.FromErr(err)
			}
			d.SetId(fmt.Sprintf("%v", resp["id"]))
			return nil
		},
		ReadContext:   func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		DeleteContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		Schema: map[string]*schema.Schema{
			"table":      {Type: schema.TypeString, Required: true},
			"name":       {Type: schema.TypeString, Required: true},
			"expression": {Type: schema.TypeString, Required: true},
			"operation":  {Type: schema.TypeString, Optional: true, Default: "ALL"},
		},
	}
}

func resourceVectorCollection() *schema.Resource {
	return &schema.Resource{
		CreateContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
			client := m.(*apiClient)
			body := map[string]interface{}{
				"name":      d.Get("name").(string),
				"dimension": d.Get("dimension").(int),
				"metric":    d.Get("metric").(string),
			}
			resp, err := client.doRequest("POST", "/v1/vectors/collections", body)
			if err != nil {
				return diag.FromErr(err)
			}
			d.SetId(fmt.Sprintf("%v", resp["id"]))
			return nil
		},
		ReadContext:   func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		DeleteContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		Schema: map[string]*schema.Schema{
			"name":      {Type: schema.TypeString, Required: true},
			"dimension": {Type: schema.TypeInt, Required: true},
			"metric":    {Type: schema.TypeString, Optional: true, Default: "cosine"},
		},
	}
}

func resourceWebhook() *schema.Resource {
	return &schema.Resource{
		CreateContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
			client := m.(*apiClient)
			body := map[string]interface{}{
				"url":    d.Get("url").(string),
				"events": d.Get("events"),
				"secret": d.Get("secret").(string),
			}
			resp, err := client.doRequest("POST", "/v1/webhooks", body)
			if err != nil {
				return diag.FromErr(err)
			}
			d.SetId(fmt.Sprintf("%v", resp["id"]))
			return nil
		},
		ReadContext:   func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		DeleteContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		Schema: map[string]*schema.Schema{
			"url":    {Type: schema.TypeString, Required: true},
			"events": {Type: schema.TypeList, Required: true, Elem: &schema.Schema{Type: schema.TypeString}},
			"secret": {Type: schema.TypeString, Optional: true, Sensitive: true},
		},
	}
}

func resourceSecret() *schema.Resource {
	return &schema.Resource{
		CreateContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
			client := m.(*apiClient)
			body := map[string]interface{}{
				"key":   d.Get("key").(string),
				"value": d.Get("value").(string),
				"scope": d.Get("scope").(string),
			}
			resp, err := client.doRequest("POST", "/v1/secrets", body)
			if err != nil {
				return diag.FromErr(err)
			}
			d.SetId(fmt.Sprintf("%v", resp["id"]))
			return nil
		},
		ReadContext:   func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		DeleteContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics { return nil },
		Schema: map[string]*schema.Schema{
			"key":   {Type: schema.TypeString, Required: true},
			"value": {Type: schema.TypeString, Required: true, Sensitive: true},
			"scope": {Type: schema.TypeString, Optional: true, Default: "global"},
		},
	}
}

func dataSourceProject() *schema.Resource {
	return &schema.Resource{
		ReadContext: func(ctx context.Context, d *schema.ResourceData, m interface{}) diag.Diagnostics {
			client := m.(*apiClient)
			resp, err := client.doRequest("GET", "/v1/project", nil)
			if err != nil {
				return diag.FromErr(err)
			}
			d.SetId(fmt.Sprintf("%v", resp["id"]))
			d.Set("name", resp["name"])
			d.Set("region", resp["region"])
			return nil
		},
		Schema: map[string]*schema.Schema{
			"name":   {Type: schema.TypeString, Computed: true},
			"region": {Type: schema.TypeString, Computed: true},
		},
	}
}
