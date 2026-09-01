//! # OpenAPI Spec Generator
//!
//! Auto-generates OpenAPI 3.1 spec from database schema, auth endpoints,
//! storage, and all registered API routes. Provides interactive REST explorer.

use crate::db::StackhouseStore;
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: ApiInfo,
    pub servers: Vec<ApiServer>,
    pub paths: HashMap<String, PathItem>,
    pub components: Components,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInfo {
    pub title: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiServer {
    pub url: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathItem {
    pub get: Option<Operation>,
    pub post: Option<Operation>,
    pub put: Option<Operation>,
    pub delete: Option<Operation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub summary: String,
    pub description: String,
    pub tags: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    pub responses: HashMap<String, Response>,
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub in_field: String,
    pub required: bool,
    pub schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    pub required: bool,
    pub content: HashMap<String, MediaType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    pub schema: Value,
    pub example: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub description: String,
    pub content: Option<HashMap<String, MediaType>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Components {
    pub schemas: HashMap<String, Value>,
    pub security_schemes: HashMap<String, SecurityScheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScheme {
    pub type_field: String,
    pub scheme: Option<String>,
    pub bearer_format: Option<String>,
}

#[derive(Clone)]
pub struct OpenApiGenerator {
    store: Arc<StackhouseStore>,
}

impl OpenApiGenerator {
    pub fn new(store: Arc<StackhouseStore>) -> Self {
        Self { store }
    }

    pub async fn generate(&self) -> StackhouseResult<OpenApiSpec> {
        let mut paths = HashMap::new();
        let mut schemas = HashMap::new();

        // Auth paths
        paths.insert("/v1/auth/signup".to_string(), PathItem {
            post: Some(Operation {
                summary: "Sign up".into(), description: "Create a new user account".into(),
                tags: vec!["Auth".into()], parameters: vec![],
                request_body: Some(RequestBody {
                    required: true,
                    content: [("application/json".into(), MediaType {
                        schema: json!({"type":"object","properties":{"email":{"type":"string"},"password":{"type":"string"}},"required":["email","password"]}),
                        example: Some(json!({"email":"user@example.com","password":"secure123"})),
                    })].into_iter().collect(),
                }),
                responses: [("201".into(), Response { description: "User created".into(), content: None })].into_iter().collect(),
                security: None,
            }),
            get: None, put: None, delete: None,
        });

        paths.insert("/v1/auth/login".to_string(), PathItem {
            post: Some(Operation {
                summary: "Log in".into(), description: "Authenticate and get tokens".into(),
                tags: vec!["Auth".into()], parameters: vec![],
                request_body: Some(RequestBody {
                    required: true,
                    content: [("application/json".into(), MediaType {
                        schema: json!({"type":"object","properties":{"email":{"type":"string"},"password":{"type":"string"}},"required":["email","password"]}),
                        example: Some(json!({"email":"user@example.com","password":"secure123"})),
                    })].into_iter().collect(),
                }),
                responses: [("200".into(), Response { description: "Tokens returned".into(), content: None })].into_iter().collect(),
                security: None,
            }),
            get: None, put: None, delete: None,
        });

        // Data paths
        paths.insert(
            "/v1/push/{collection}".to_string(),
            PathItem {
                post: Some(Operation {
                    summary: "Push data".into(),
                    description: "Insert or update a document".into(),
                    tags: vec!["Data".into()],
                    parameters: vec![Parameter {
                        name: "collection".into(),
                        in_field: "path".into(),
                        required: true,
                        schema: json!({"type":"string"}),
                    }],
                    request_body: Some(RequestBody {
                        required: true,
                        content: [(
                            "application/json".into(),
                            MediaType {
                                schema: json!({"type":"object"}),
                                example: Some(json!({"title":"Hello","count":42})),
                            },
                        )]
                        .into_iter()
                        .collect(),
                    }),
                    responses: [(
                        "201".into(),
                        Response {
                            description: "Document created".into(),
                            content: None,
                        },
                    )]
                    .into_iter()
                    .collect(),
                    security: Some(vec![[("bearerAuth".into(), vec![])].into_iter().collect()]),
                }),
                get: None,
                put: None,
                delete: None,
            },
        );

        paths.insert(
            "/v1/query/{collection}".to_string(),
            PathItem {
                get: Some(Operation {
                    summary: "Query data".into(),
                    description: "Query documents with filters".into(),
                    tags: vec!["Data".into()],
                    parameters: vec![
                        Parameter {
                            name: "collection".into(),
                            in_field: "path".into(),
                            required: true,
                            schema: json!({"type":"string"}),
                        },
                        Parameter {
                            name: "filter".into(),
                            in_field: "query".into(),
                            required: false,
                            schema: json!({"type":"string"}),
                        },
                        Parameter {
                            name: "limit".into(),
                            in_field: "query".into(),
                            required: false,
                            schema: json!({"type":"integer","default":100}),
                        },
                    ],
                    request_body: None,
                    responses: [(
                        "200".into(),
                        Response {
                            description: "Query results".into(),
                            content: None,
                        },
                    )]
                    .into_iter()
                    .collect(),
                    security: Some(vec![[("bearerAuth".into(), vec![])].into_iter().collect()]),
                }),
                post: None,
                put: None,
                delete: None,
            },
        );

        // Add table-generated endpoints
        if let Ok(tables) = self.get_tables().await {
            for table in tables {
                let path = format!("/v1/tables/{}", table);
                paths.insert(
                    path.clone(),
                    PathItem {
                        get: Some(Operation {
                            summary: format!("List {}", table),
                            description: format!("Query {} table", table),
                            tags: vec!["Auto-REST".into()],
                            parameters: vec![
                                Parameter {
                                    name: "filter".into(),
                                    in_field: "query".into(),
                                    required: false,
                                    schema: json!({"type":"string"}),
                                },
                                Parameter {
                                    name: "limit".into(),
                                    in_field: "query".into(),
                                    required: false,
                                    schema: json!({"type":"integer"}),
                                },
                            ],
                            request_body: None,
                            responses: [(
                                "200".into(),
                                Response {
                                    description: "OK".into(),
                                    content: None,
                                },
                            )]
                            .into_iter()
                            .collect(),
                            security: Some(vec![[("bearerAuth".into(), vec![])]
                                .into_iter()
                                .collect()]),
                        }),
                        post: Some(Operation {
                            summary: format!("Create {}", table),
                            description: format!("Insert into {}", table),
                            tags: vec!["Auto-REST".into()],
                            parameters: vec![],
                            request_body: Some(RequestBody {
                                required: true,
                                content: [(
                                    "application/json".into(),
                                    MediaType {
                                        schema: json!({"type":"object"}),
                                        example: None,
                                    },
                                )]
                                .into_iter()
                                .collect(),
                            }),
                            responses: [(
                                "201".into(),
                                Response {
                                    description: "Created".into(),
                                    content: None,
                                },
                            )]
                            .into_iter()
                            .collect(),
                            security: Some(vec![[("bearerAuth".into(), vec![])]
                                .into_iter()
                                .collect()]),
                        }),
                        put: None,
                        delete: None,
                    },
                );
            }
        }

        schemas.insert(
            "User".into(),
            json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "email": {"type": "string"},
                    "created_at": {"type": "string", "format": "date-time"},
                }
            }),
        );

        let spec = OpenApiSpec {
            openapi: "3.1.0".to_string(),
            info: ApiInfo {
                title: "Stackhouse API".into(),
                version: "1.0.0".into(),
                description:
                    "Auto-generated REST API for Stackhouse — schema-later database platform".into(),
            },
            servers: vec![ApiServer {
                url: "http://localhost:3000".into(),
                description: "Local development".into(),
            }],
            paths,
            components: Components {
                schemas,
                security_schemes: [(
                    "bearerAuth".into(),
                    SecurityScheme {
                        type_field: "http".into(),
                        scheme: Some("bearer".into()),
                        bearer_format: Some("JWT".into()),
                    },
                )]
                .into_iter()
                .collect(),
            },
        };

        info!("📘 OpenAPI spec generated with {} paths", spec.paths.len());
        Ok(spec)
    }

    async fn get_tables(&self) -> StackhouseResult<Vec<String>> {
        let rows = self.store.query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name LIKE 'stackhouse_%'".to_string(),
            vec![],
        ).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                r.iter()
                    .find(|(k, _)| k == "table_name")
                    .and_then(|(_, v)| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect())
    }

    pub fn to_json(&self, spec: &OpenApiSpec) -> StackhouseResult<Value> {
        serde_json::to_value(spec)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("OpenAPI serialization: {}", e)))
    }
}
