use crate::auth::AuthActor;
use crate::error::HttpApiError;
use arc_swap::ArcSwap;
use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputObject, InputValue, Object, Scalar, Schema, TypeRef,
};
use async_graphql::{Error as GqlError, Request as GqlRequest, Variables};
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use cms_core::{
    Actor, ApiError, ApiResult, ContentKind, ContentTypeSchema, DocumentQuery, DocumentStatus,
    FieldDefinition, FieldType, InvalidationEvent, MediaListFilter, PaginationMeta, PopulateSpec,
    RelationKind, SortSpec,
};
use cms_services::ServiceContainer;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct DynamicGraphqlEngine {
    schema: Arc<ArcSwap<Schema>>,
    dirty: Arc<AtomicBool>,
    services: ServiceContainer,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlHttpBody {
    pub query: String,
    #[serde(default)]
    pub operation_name: Option<String>,
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
}

impl DynamicGraphqlEngine {
    pub fn new(services: ServiceContainer) -> Self {
        let placeholder_query = Object::new("Query").field(Field::new(
            "_version",
            TypeRef::named_nn(TypeRef::STRING),
            |_| {
                FieldFuture::new(async move {
                    Ok(Some(async_graphql::Value::from(env!("CARGO_PKG_VERSION"))))
                })
            },
        ));
        let initial = Schema::build("Query", None, None)
            .register(placeholder_query)
            .finish()
            .expect("minimal bootstrap GraphQL schema");

        let holder = Arc::new(ArcSwap::from_pointee(initial));
        let dirty = Arc::new(AtomicBool::new(true));

        // Subscribe to `SchemaUpdated` pubsub events to mark schema dirty for zero-redeploy hot-swap
        let mut rx = services.pubsub.subscribe_local();
        let bg_dirty = dirty.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(InvalidationEvent::SchemaUpdated { .. }) => {
                        bg_dirty.store(true, Ordering::Release);
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        bg_dirty.store(true, Ordering::Release);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Self {
            schema: holder,
            dirty,
            services,
        }
    }

    pub async fn ensure_fresh_schema(&self) -> ApiResult<Arc<Schema>> {
        if self.dirty.swap(false, Ordering::AcqRel) {
            match build_dynamic_schema(&self.services).await {
                Ok(rebuilt) => {
                    self.schema.store(Arc::new(rebuilt));
                }
                Err(err) => {
                    self.dirty.store(true, Ordering::Release);
                    return Err(err);
                }
            }
        }
        Ok(self.schema.load_full())
    }
}

pub fn graphql_routes(engine: DynamicGraphqlEngine) -> Router<ServiceContainer> {
    Router::new()
        .route(
            "/graphql",
            get(graphql_playground_handler).post(graphql_execute_handler),
        )
        .layer(Extension(engine))
}

async fn graphql_playground_handler(
    State(services): State<ServiceContainer>,
) -> Result<Response, HttpApiError> {
    if !services.config.enable_docs {
        return Err(HttpApiError(ApiError::NotFound(
            "GraphQL explorer is disabled in production".into(),
        )));
    }
    let html = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>RAPI GraphQL Explorer</title>
<style>
  body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; background: #09090b; color: #f4f4f5; display: flex; flex-direction: column; height: 100vh; }
  header { padding: 10px 16px; border-bottom: 1px solid #27272a; display: flex; align-items: center; justify-content: space-between; font-size: 13px; }
  main { flex: 1; display: grid; grid-template-columns: 1fr 1fr; gap: 1px; background: #27272a; }
  textarea, pre { margin: 0; padding: 16px; background: #09090b; color: #e4e4e7; border: 0; font-family: ui-monospace, monospace; font-size: 12px; resize: none; outline: none; overflow: auto; }
  button { background: #4f46e5; color: white; border: 0; padding: 6px 14px; border-radius: 5px; font-size: 12px; font-weight: 600; cursor: pointer; }
  input { background: #18181b; border: 1px solid #3f3f46; color: #e4e4e7; padding: 5px 10px; border-radius: 4px; font-family: monospace; font-size: 11px; width: 280px; }
</style>
</head>
<body>
  <header>
    <div><strong>RAPI GraphQL</strong> <span style="color:#71717a;font-family:monospace">/graphql (Strapi v5 dynamic schema)</span></div>
    <div style="display:flex;gap:8px;align-items:center">
      <input id="tok" placeholder="Bearer token (optional)" />
      <button onclick="run()">Run Query</button>
    </div>
  </header>
  <main>
    <textarea id="q">query {
  i18NLocales
}</textarea>
    <pre id="out">// Click "Run Query" to execute against /graphql</pre>
  </main>
  <script>
    async function run() {
      const tok = document.getElementById('tok').value.trim();
      const headers = { 'Content-Type': 'application/json' };
      if (tok) headers['Authorization'] = tok.startsWith('Bearer ') ? tok : 'Bearer ' + tok;
      const res = await fetch('/graphql', {
        method: 'POST',
        headers,
        body: JSON.stringify({ query: document.getElementById('q').value })
      });
      const json = await res.json();
      document.getElementById('out').textContent = JSON.stringify(json, null, 2);
    }
  </script>
</body>
</html>"#;
    Ok(Html(html).into_response())
}

async fn graphql_execute_handler(
    Extension(engine): Extension<DynamicGraphqlEngine>,
    AuthActor(actor): AuthActor,
    Json(body): Json<GraphqlHttpBody>,
) -> Json<serde_json::Value> {
    let schema = match engine.ensure_fresh_schema().await {
        Ok(s) => s,
        Err(e) => {
            return Json(serde_json::json!({
                "data": null,
                "errors": [{ "message": format!("GraphQL schema build error: {e}") }]
            }));
        }
    };
    let mut gql_req = GqlRequest::new(body.query);
    if let Some(op) = body.operation_name {
        gql_req = gql_req.operation_name(op);
    }
    if let Some(vars) = body.variables {
        gql_req = gql_req.variables(Variables::from_json(vars));
    }
    gql_req = gql_req.data(actor);

    let resp = schema.execute(gql_req).await;
    let serialized = serde_json::to_value(&resp).unwrap_or_else(|e| {
        serde_json::json!({
            "data": null,
            "errors": [{ "message": format!("GraphQL serialization error: {e}") }]
        })
    });
    Json(serialized)
}

// ============================================================================
// Dynamic Strapi v5 Schema Builder
// ============================================================================

async fn build_dynamic_schema(services: &ServiceContainer) -> ApiResult<Schema> {
    let all_schemas = services.schema.list_schemas().await?;
    let content_schemas: Vec<ContentTypeSchema> = all_schemas
        .into_iter()
        .filter(|s| s.kind != ContentKind::Component)
        .collect();

    let mut type_names_by_uid: HashMap<String, String> = HashMap::new();
    for s in &content_schemas {
        type_names_by_uid.insert(s.uid.clone(), pascal_case(&s.singular_name));
    }

    let mut builder = Schema::build("Query", Some("Mutation"), None)
        .limit_depth(12)
        .limit_complexity(500);
    if !services.config.enable_docs {
        builder = builder.disable_introspection();
    }
    builder = builder
        .register(Scalar::new("JSON"))
        .register(Scalar::new("DateTime"))
        .register(Scalar::new("Date"))
        .register(Scalar::new("Time"))
        .register(Scalar::new("Long"));

    // Shared `PaginationArg` input & `PaginationInfo` + `DeleteMutationResponse` output types
    let pagination_input = InputObject::new("PaginationArg")
        .field(InputValue::new("page", TypeRef::named(TypeRef::INT)))
        .field(InputValue::new("pageSize", TypeRef::named(TypeRef::INT)))
        .field(InputValue::new("start", TypeRef::named(TypeRef::INT)))
        .field(InputValue::new("limit", TypeRef::named(TypeRef::INT)));
    builder = builder.register(pagination_input);

    let page_info_obj = Object::new("PaginationInfo")
        .field(Field::new(
            "page",
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let meta = ctx.parent_value.try_downcast_ref::<PaginationMeta>()?;
                    Ok(Some(async_graphql::Value::from(meta.page as i64)))
                })
            },
        ))
        .field(Field::new(
            "pageSize",
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let meta = ctx.parent_value.try_downcast_ref::<PaginationMeta>()?;
                    Ok(Some(async_graphql::Value::from(meta.page_size as i64)))
                })
            },
        ))
        .field(Field::new(
            "pageCount",
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let meta = ctx.parent_value.try_downcast_ref::<PaginationMeta>()?;
                    Ok(Some(async_graphql::Value::from(meta.page_count as i64)))
                })
            },
        ))
        .field(Field::new(
            "total",
            TypeRef::named_nn(TypeRef::INT),
            |ctx| {
                FieldFuture::new(async move {
                    let meta = ctx.parent_value.try_downcast_ref::<PaginationMeta>()?;
                    Ok(Some(async_graphql::Value::from(meta.total as i64)))
                })
            },
        ));
    builder = builder.register(page_info_obj);

    let delete_resp_obj = Object::new("DeleteMutationResponse").field(Field::new(
        "documentId",
        TypeRef::named_nn(TypeRef::ID),
        |ctx| {
            FieldFuture::new(async move {
                let doc_id = ctx.parent_value.try_downcast_ref::<String>()?;
                Ok(Some(async_graphql::Value::from(doc_id.clone())))
            })
        },
    ));
    builder = builder.register(delete_resp_obj);

    // Built-in Query & Mutation roots
    let svc_locales = services.clone();
    let svc_files = services.clone();
    let mut query_root = Object::new("Query")
        .field(Field::new(
            "_version",
            TypeRef::named_nn(TypeRef::STRING),
            |_| {
                FieldFuture::new(async move {
                    Ok(Some(async_graphql::Value::from(env!("CARGO_PKG_VERSION"))))
                })
            },
        ))
        .field(Field::new(
            "i18NLocales",
            TypeRef::named_nn_list_nn("JSON"),
            move |_| {
                let svc = svc_locales.clone();
                FieldFuture::new(async move {
                    let locales = svc
                        .governance
                        .list_locales()
                        .await
                        .map_err(api_err_to_gql)?;
                    let vals: Vec<FieldValue<'static>> = locales
                        .into_iter()
                        .filter_map(|l| serde_json::to_value(l).ok())
                        .filter_map(|j| async_graphql::Value::from_json(j).ok())
                        .map(FieldValue::value)
                        .collect();
                    Ok(Some(FieldValue::list(vals)))
                })
            },
        ))
        .field(
            Field::new(
                "uploadFiles",
                TypeRef::named_nn_list_nn("JSON"),
                move |ctx| {
                    let svc = svc_files.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        if !actor.can("plugin::upload.read", None)
                            && !actor.can("api::upload.find", None)
                        {
                            return Err(GqlError::new("Forbidden: missing upload read permission"));
                        }
                        let page = ctx
                            .args
                            .get("page")
                            .and_then(|v| v.i64().ok())
                            .unwrap_or(1)
                            .max(1) as u64;
                        let page_size = ctx
                            .args
                            .get("pageSize")
                            .and_then(|v| v.i64().ok())
                            .unwrap_or(24)
                            .clamp(1, 100) as u64;
                        let q = ctx
                            .args
                            .get("q")
                            .and_then(|v| v.string().ok().map(String::from));
                        let (files, _) = svc
                            .media
                            .list_files(&MediaListFilter {
                                folder_id: None,
                                mime_prefix: None,
                                search_query: q,
                                page,
                                page_size,
                            })
                            .await
                            .map_err(api_err_to_gql)?;
                        let vals: Vec<FieldValue<'static>> = files
                            .into_iter()
                            .filter_map(|f| serde_json::to_value(f).ok())
                            .filter_map(|j| async_graphql::Value::from_json(j).ok())
                            .map(FieldValue::value)
                            .collect();
                        Ok(Some(FieldValue::list(vals)))
                    })
                },
            )
            .argument(InputValue::new("page", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("pageSize", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("q", TypeRef::named(TypeRef::STRING))),
        );

    let mut mutation_root = Object::new("Mutation").field(Field::new(
        "_noop",
        TypeRef::named_nn(TypeRef::BOOLEAN),
        |_| FieldFuture::new(async move { Ok(Some(async_graphql::Value::from(true))) }),
    ));

    // Register dynamic types, queries, connections, and mutations per content type
    for schema in &content_schemas {
        let type_name = type_names_by_uid
            .get(&schema.uid)
            .cloned()
            .unwrap_or_else(|| pascal_case(&schema.singular_name));

        let gql_obj = build_entity_object(&type_name, schema, &type_names_by_uid);
        builder = builder.register(gql_obj);

        if schema.kind == ContentKind::CollectionType {
            // `<PascalSingular>EntityResponseCollection`
            let conn_type_name = format!("{type_name}EntityResponseCollection");
            let conn_obj = Object::new(&conn_type_name)
                .field(Field::new(
                    "nodes",
                    TypeRef::named_nn_list_nn(&type_name),
                    |ctx| {
                        FieldFuture::new(async move {
                            let pair = ctx
                                .parent_value
                                .try_downcast_ref::<(Vec<serde_json::Value>, PaginationMeta)>()?;
                            let items: Vec<FieldValue<'static>> = pair
                                .0
                                .iter()
                                .cloned()
                                .map(FieldValue::owned_any)
                                .collect();
                            Ok(Some(FieldValue::list(items)))
                        })
                    },
                ))
                .field(Field::new(
                    "pageInfo",
                    TypeRef::named_nn("PaginationInfo"),
                    |ctx| {
                        FieldFuture::new(async move {
                            let pair = ctx
                                .parent_value
                                .try_downcast_ref::<(Vec<serde_json::Value>, PaginationMeta)>()?;
                            Ok(Some(FieldValue::owned_any(pair.1.clone())))
                        })
                    },
                ));
            builder = builder.register(conn_obj);

            // 1. `<pluralApiId>(filters, sort, pagination, locale, status)`
            let q_schema = schema.clone();
            let q_svc = services.clone();
            let list_field = add_collection_query_args(Field::new(
                &schema.plural_name,
                TypeRef::named_nn_list_nn(&type_name),
                move |ctx| {
                    let s = q_schema.clone();
                    let svc = q_svc.clone();
                    FieldFuture::new(async move {
                        let (items, _) = execute_gql_collection_query(&ctx, &svc, &s).await?;
                        let out: Vec<FieldValue<'static>> =
                            items.into_iter().map(FieldValue::owned_any).collect();
                        Ok(Some(FieldValue::list(out)))
                    })
                },
            ));
            query_root = query_root.field(list_field);

            // 2. `<pluralApiId>_connection(...)`
            let c_schema = schema.clone();
            let c_svc = services.clone();
            let conn_field_name = format!("{}_connection", schema.plural_name);
            let conn_field = add_collection_query_args(Field::new(
                conn_field_name,
                TypeRef::named_nn(&conn_type_name),
                move |ctx| {
                    let s = c_schema.clone();
                    let svc = c_svc.clone();
                    FieldFuture::new(async move {
                        let pair = execute_gql_collection_query(&ctx, &svc, &s).await?;
                        Ok(Some(FieldValue::owned_any(pair)))
                    })
                },
            ));
            query_root = query_root.field(conn_field);

            // 3. `<singularApiId>(documentId: ID!, locale, status)`
            let one_schema = schema.clone();
            let one_svc = services.clone();
            let find_one_field = Field::new(
                &schema.singular_name,
                TypeRef::named(&type_name),
                move |ctx| {
                    let s = one_schema.clone();
                    let svc = one_svc.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        enforce_gql_perm(actor, &format!("{}.findOne", s.uid), Some(&s.uid))?;
                        let doc_id = ctx.args.try_get("documentId")?.string()?.to_string();
                        let status = extract_gql_status(&ctx, actor)?;
                        let req_loc = ctx
                            .args
                            .get("locale")
                            .and_then(|v| v.string().ok().map(String::from));
                        let locale = svc
                            .governance
                            .resolve_and_validate_locale(
                                s.i18n_enabled,
                                req_loc.as_deref(),
                                false,
                            )
                            .await
                            .map_err(api_err_to_gql)?;
                        match svc
                            .content
                            .find_one(
                                &s,
                                &doc_id,
                                &locale,
                                status,
                                &PopulateSpec::All,
                                &[],
                                !actor.is_admin(),
                            )
                            .await
                        {
                            Ok(doc) => Ok(Some(FieldValue::owned_any(doc))),
                            Err(ApiError::NotFound(_)) => Ok(None),
                            Err(other) => Err(api_err_to_gql(other)),
                        }
                    })
                },
            )
            .argument(InputValue::new(
                "documentId",
                TypeRef::named_nn(TypeRef::ID),
            ))
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)));
            query_root = query_root.field(find_one_field);

            // 4. `create<PascalSingular>(data: JSON!, status, locale)`
            let cr_schema = schema.clone();
            let cr_svc = services.clone();
            let create_mut_name = format!("create{type_name}");
            let create_field = Field::new(
                create_mut_name,
                TypeRef::named_nn(&type_name),
                move |ctx| {
                    let s = cr_schema.clone();
                    let svc = cr_svc.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        enforce_gql_perm(actor, &format!("{}.create", s.uid), Some(&s.uid))?;
                        let data_map = extract_gql_data_map(&ctx)?;
                        let target_status = extract_gql_write_status(&ctx, DocumentStatus::Published)?;
                        if s.draft_and_publish && target_status == DocumentStatus::Published {
                            svc.governance
                                .enforce_new_entry_publish_gate()
                                .await
                                .map_err(api_err_to_gql)?;
                        }
                        let req_loc = ctx
                            .args
                            .get("locale")
                            .and_then(|v| v.string().ok().map(String::from));
                        let locale = svc
                            .governance
                            .resolve_and_validate_locale(s.i18n_enabled, req_loc.as_deref(), true)
                            .await
                            .map_err(api_err_to_gql)?;
                        let created = svc
                            .content
                            .create_entry(
                                &s,
                                &locale,
                                target_status,
                                &data_map,
                                actor_id_opt(actor),
                                !actor.is_admin(),
                            )
                            .await
                            .map_err(api_err_to_gql)?;
                        svc.integrations
                            .emit_webhook_event("entry.create", Some(&s.uid), created.clone())
                            .await;
                        Ok(Some(FieldValue::owned_any(created)))
                    })
                },
            )
            .argument(InputValue::new("data", TypeRef::named_nn("JSON")))
            .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)));
            mutation_root = mutation_root.field(create_field);

            // 5. `update<PascalSingular>(documentId: ID!, data: JSON!, status, locale)`
            let up_schema = schema.clone();
            let up_svc = services.clone();
            let update_mut_name = format!("update{type_name}");
            let update_field = Field::new(
                update_mut_name,
                TypeRef::named_nn(&type_name),
                move |ctx| {
                    let s = up_schema.clone();
                    let svc = up_svc.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        enforce_gql_perm(actor, &format!("{}.update", s.uid), Some(&s.uid))?;
                        let doc_id = ctx.args.try_get("documentId")?.string()?.to_string();
                        let data_map = extract_gql_data_map(&ctx)?;
                        let target_status = extract_gql_write_status(&ctx, DocumentStatus::Published)?;
                        let req_loc = ctx
                            .args
                            .get("locale")
                            .and_then(|v| v.string().ok().map(String::from));
                        let locale = svc
                            .governance
                            .resolve_and_validate_locale(s.i18n_enabled, req_loc.as_deref(), true)
                            .await
                            .map_err(api_err_to_gql)?;
                        if s.draft_and_publish && target_status == DocumentStatus::Published {
                            svc.governance
                                .enforce_publish_workflow_gate(&doc_id, &locale)
                                .await
                                .map_err(api_err_to_gql)?;
                        }
                        let updated = svc
                            .content
                            .update_entry(
                                &s,
                                &doc_id,
                                &locale,
                                target_status,
                                &data_map,
                                actor_id_opt(actor),
                                !actor.is_admin(),
                            )
                            .await
                            .map_err(api_err_to_gql)?;
                        svc.integrations
                            .emit_webhook_event("entry.update", Some(&s.uid), updated.clone())
                            .await;
                        Ok(Some(FieldValue::owned_any(updated)))
                    })
                },
            )
            .argument(InputValue::new(
                "documentId",
                TypeRef::named_nn(TypeRef::ID),
            ))
            .argument(InputValue::new("data", TypeRef::named_nn("JSON")))
            .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)));
            mutation_root = mutation_root.field(update_field);

            // 6. `delete<PascalSingular>(documentId: ID!, locale)`
            let del_schema = schema.clone();
            let del_svc = services.clone();
            let delete_mut_name = format!("delete{type_name}");
            let delete_field = Field::new(
                delete_mut_name,
                TypeRef::named_nn("DeleteMutationResponse"),
                move |ctx| {
                    let s = del_schema.clone();
                    let svc = del_svc.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        enforce_gql_perm(actor, &format!("{}.delete", s.uid), Some(&s.uid))?;
                        let doc_id = ctx.args.try_get("documentId")?.string()?.to_string();
                        let req_loc = ctx
                            .args
                            .get("locale")
                            .and_then(|v| v.string().ok().map(String::from));
                        svc.content
                            .delete_entry(&s, &doc_id, req_loc.as_deref())
                            .await
                            .map_err(api_err_to_gql)?;
                        svc.integrations
                            .emit_webhook_event(
                                "entry.delete",
                                Some(&s.uid),
                                serde_json::json!({ "documentId": doc_id }),
                            )
                            .await;
                        Ok(Some(FieldValue::owned_any(doc_id)))
                    })
                },
            )
            .argument(InputValue::new(
                "documentId",
                TypeRef::named_nn(TypeRef::ID),
            ))
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)));
            mutation_root = mutation_root.field(delete_field);
        } else if schema.kind == ContentKind::SingleType {
            // SingleType Query `<singularApiId>(locale, status)`
            let sq_schema = schema.clone();
            let sq_svc = services.clone();
            let single_q = Field::new(
                &schema.singular_name,
                TypeRef::named(&type_name),
                move |ctx| {
                    let s = sq_schema.clone();
                    let svc = sq_svc.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        enforce_gql_perm(actor, &format!("{}.find", s.uid), Some(&s.uid))?;
                        let status = extract_gql_status(&ctx, actor)?;
                        let req_loc = ctx
                            .args
                            .get("locale")
                            .and_then(|v| v.string().ok().map(String::from));
                        let locale = svc
                            .governance
                            .resolve_and_validate_locale(
                                s.i18n_enabled,
                                req_loc.as_deref(),
                                false,
                            )
                            .await
                            .map_err(api_err_to_gql)?;
                        let doc_opt = svc
                            .content
                            .find_single(
                                &s,
                                &locale,
                                status,
                                &PopulateSpec::All,
                                &[],
                                !actor.is_admin(),
                            )
                            .await
                            .map_err(api_err_to_gql)?;
                        Ok(doc_opt.map(FieldValue::owned_any))
                    })
                },
            )
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)));
            query_root = query_root.field(single_q);

            // SingleType `update<PascalSingular>(data: JSON!, status, locale)`
            let su_schema = schema.clone();
            let su_svc = services.clone();
            let single_up = Field::new(
                format!("update{type_name}"),
                TypeRef::named_nn(&type_name),
                move |ctx| {
                    let s = su_schema.clone();
                    let svc = su_svc.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        enforce_gql_perm(actor, &format!("{}.update", s.uid), Some(&s.uid))?;
                        let data_map = extract_gql_data_map(&ctx)?;
                        let target_status = extract_gql_write_status(&ctx, DocumentStatus::Published)?;
                        let req_loc = ctx
                            .args
                            .get("locale")
                            .and_then(|v| v.string().ok().map(String::from));
                        let locale = svc
                            .governance
                            .resolve_and_validate_locale(s.i18n_enabled, req_loc.as_deref(), true)
                            .await
                            .map_err(api_err_to_gql)?;
                        if s.draft_and_publish && target_status == DocumentStatus::Published {
                            if let Some(existing) = svc
                                .content
                                .find_single(
                                    &s,
                                    &locale,
                                    DocumentStatus::Draft,
                                    &PopulateSpec::None,
                                    &[],
                                    false,
                                )
                                .await
                                .map_err(api_err_to_gql)?
                            {
                                if let Some(did) =
                                    existing.get("documentId").and_then(|v| v.as_str())
                                {
                                    svc.governance
                                        .enforce_publish_workflow_gate(did, &locale)
                                        .await
                                        .map_err(api_err_to_gql)?;
                                }
                            } else {
                                svc.governance
                                    .enforce_new_entry_publish_gate()
                                    .await
                                    .map_err(api_err_to_gql)?;
                            }
                        }
                        let updated = svc
                            .content
                            .upsert_single(
                                &s,
                                &locale,
                                target_status,
                                &data_map,
                                actor_id_opt(actor),
                                !actor.is_admin(),
                            )
                            .await
                            .map_err(api_err_to_gql)?;
                        svc.integrations
                            .emit_webhook_event("entry.update", Some(&s.uid), updated.clone())
                            .await;
                        Ok(Some(FieldValue::owned_any(updated)))
                    })
                },
            )
            .argument(InputValue::new("data", TypeRef::named_nn("JSON")))
            .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)));
            mutation_root = mutation_root.field(single_up);

            // SingleType `delete<PascalSingular>(locale)`
            let sd_schema = schema.clone();
            let sd_svc = services.clone();
            let single_del = Field::new(
                format!("delete{type_name}"),
                TypeRef::named_nn("DeleteMutationResponse"),
                move |ctx| {
                    let s = sd_schema.clone();
                    let svc = sd_svc.clone();
                    FieldFuture::new(async move {
                        let actor = ctx.data::<Actor>()?;
                        enforce_gql_perm(actor, &format!("{}.delete", s.uid), Some(&s.uid))?;
                        let req_loc = ctx
                            .args
                            .get("locale")
                            .and_then(|v| v.string().ok().map(String::from));
                        let locale = svc
                            .governance
                            .resolve_and_validate_locale(s.i18n_enabled, req_loc.as_deref(), false)
                            .await
                            .map_err(api_err_to_gql)?;
                        let check_status = if s.draft_and_publish {
                            DocumentStatus::Draft
                        } else {
                            DocumentStatus::Published
                        };
                        let existing = svc
                            .content
                            .find_single(&s, &locale, check_status, &PopulateSpec::None, &[], false)
                            .await
                            .map_err(api_err_to_gql)?
                            .ok_or_else(|| GqlError::new("Single type document not found"))?;
                        let doc_id = existing["documentId"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        svc.content
                            .delete_entry(&s, &doc_id, req_loc.as_deref())
                            .await
                            .map_err(api_err_to_gql)?;
                        svc.integrations
                            .emit_webhook_event(
                                "entry.delete",
                                Some(&s.uid),
                                serde_json::json!({ "documentId": doc_id }),
                            )
                            .await;
                        Ok(Some(FieldValue::owned_any(doc_id)))
                    })
                },
            )
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)));
            mutation_root = mutation_root.field(single_del);
        }
    }

    builder = builder.register(query_root).register(mutation_root);
    builder
        .finish()
        .map_err(|e| ApiError::Internal(format!("Build dynamic GraphQL schema failed: {e}")))
}

fn build_entity_object(
    type_name: &str,
    schema: &ContentTypeSchema,
    type_names_by_uid: &HashMap<String, String>,
) -> Object {
    let mut obj = Object::new(type_name)
        .field(json_scalar_field("documentId", TypeRef::named_nn(TypeRef::ID)))
        .field(json_scalar_field(
            "locale",
            TypeRef::named_nn(TypeRef::STRING),
        ))
        .field(json_scalar_field("publishedAt", TypeRef::named("DateTime")))
        .field(json_scalar_field("createdAt", TypeRef::named_nn("DateTime")))
        .field(json_scalar_field("updatedAt", TypeRef::named_nn("DateTime")));

    for f in schema.fields.iter().filter(|f| !f.private) {
        if f.field_type == FieldType::Relation {
            if let Some(rel) = &f.relation {
                if let Some(target_type) = rel
                    .target_uid
                    .as_deref()
                    .and_then(|u| type_names_by_uid.get(u))
                {
                    let is_single = matches!(
                        rel.relation_type,
                        RelationKind::OneToOne | RelationKind::ManyToOne | RelationKind::MorphOne
                    );
                    let fname = f.name.clone();
                    if is_single {
                        obj = obj.field(Field::new(
                            &f.name,
                            TypeRef::named(target_type),
                            move |ctx| {
                                let key = fname.clone();
                                FieldFuture::new(async move {
                                    let parent =
                                        ctx.parent_value.try_downcast_ref::<serde_json::Value>()?;
                                    match parent.get(&key) {
                                        Some(v) if v.is_object() => {
                                            Ok(Some(FieldValue::owned_any(v.clone())))
                                        }
                                        _ => Ok(None),
                                    }
                                })
                            },
                        ));
                    } else {
                        obj = obj.field(Field::new(
                            &f.name,
                            TypeRef::named_nn_list_nn(target_type),
                            move |ctx| {
                                let key = fname.clone();
                                FieldFuture::new(async move {
                                    let parent =
                                        ctx.parent_value.try_downcast_ref::<serde_json::Value>()?;
                                    let arr = parent
                                        .get(&key)
                                        .and_then(|v| v.as_array())
                                        .cloned()
                                        .unwrap_or_default();
                                    let list: Vec<FieldValue<'static>> =
                                        arr.into_iter().map(FieldValue::owned_any).collect();
                                    Ok(Some(FieldValue::list(list)))
                                })
                            },
                        ));
                    }
                    continue;
                }
            }
        }

        let tref = graphql_type_ref_for_field(f);
        obj = obj.field(json_scalar_field(&f.name, tref));
    }

    obj
}

fn json_scalar_field(name: &str, type_ref: TypeRef) -> Field {
    let key = name.to_string();
    Field::new(name, type_ref, move |ctx| {
        let k = key.clone();
        FieldFuture::new(async move {
            let parent = ctx.parent_value.try_downcast_ref::<serde_json::Value>()?;
            match parent.get(&k) {
                None | Some(serde_json::Value::Null) => Ok(None),
                Some(val) => {
                    let gql_val = async_graphql::Value::from_json(val.clone())
                        .map_err(|e| GqlError::new(format!("JSON->GraphQL value error: {e}")))?;
                    Ok(Some(FieldValue::value(gql_val)))
                }
            }
        })
    })
}

fn graphql_type_ref_for_field(f: &FieldDefinition) -> TypeRef {
    match f.field_type {
        FieldType::String
        | FieldType::Text
        | FieldType::RichText
        | FieldType::Email
        | FieldType::Enumeration
        | FieldType::Uid
        | FieldType::Password => TypeRef::named(TypeRef::STRING),
        FieldType::Integer => TypeRef::named(TypeRef::INT),
        FieldType::BigInteger => TypeRef::named("Long"),
        FieldType::Float | FieldType::Decimal => TypeRef::named(TypeRef::FLOAT),
        FieldType::Boolean => TypeRef::named(TypeRef::BOOLEAN),
        FieldType::Date => TypeRef::named("Date"),
        FieldType::DateTime => TypeRef::named("DateTime"),
        FieldType::Time => TypeRef::named("Time"),
        FieldType::Json
        | FieldType::Blocks
        | FieldType::Media
        | FieldType::Relation
        | FieldType::Component
        | FieldType::DynamicZone
        | FieldType::Custom => TypeRef::named("JSON"),
    }
}

fn add_collection_query_args(field: Field) -> Field {
    field
        .argument(InputValue::new("filters", TypeRef::named("JSON")))
        .argument(InputValue::new(
            "sort",
            TypeRef::named_list(TypeRef::STRING),
        ))
        .argument(InputValue::new(
            "pagination",
            TypeRef::named("PaginationArg"),
        ))
        .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING)))
        .argument(InputValue::new("status", TypeRef::named(TypeRef::STRING)))
}

async fn execute_gql_collection_query(
    ctx: &async_graphql::dynamic::ResolverContext<'_>,
    services: &ServiceContainer,
    schema: &ContentTypeSchema,
) -> Result<(Vec<serde_json::Value>, PaginationMeta), GqlError> {
    let actor = ctx.data::<Actor>()?;
    enforce_gql_perm(actor, &format!("{}.find", schema.uid), Some(&schema.uid))?;

    let status = extract_gql_status(ctx, actor)?;
    let req_loc = ctx
        .args
        .get("locale")
        .and_then(|v| v.string().ok().map(String::from));
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, req_loc.as_deref(), false)
        .await
        .map_err(api_err_to_gql)?;

    let filters = ctx
        .args
        .get("filters")
        .and_then(|v| v.as_value().clone().into_json().ok());

    let mut sort = Vec::new();
    if let Some(sort_list) = ctx.args.get("sort").and_then(|v| v.list().ok()) {
        for item in sort_list.iter() {
            if let Ok(s) = item.string() {
                let (field, desc) = match s.split_once(':') {
                    Some((f, dir)) => (f.trim().to_string(), dir.eq_ignore_ascii_case("desc")),
                    None => (s.trim().to_string(), false),
                };
                if !field.is_empty() {
                    sort.push(SortSpec { field, desc });
                }
            }
        }
    }

    let mut page = 1_u64;
    let mut page_size = 25_u64;
    let mut start = None;
    let mut limit = None;
    if let Some(pag_obj) = ctx.args.get("pagination").and_then(|v| v.object().ok()) {
        if let Some(p) = pag_obj.get("page").and_then(|v| v.i64().ok()) {
            page = p.max(1) as u64;
        }
        if let Some(ps) = pag_obj.get("pageSize").and_then(|v| v.i64().ok()) {
            page_size = ps.clamp(1, 100) as u64;
        }
        if let Some(st) = pag_obj.get("start").and_then(|v| v.i64().ok()) {
            start = Some(st.max(0) as u64);
        }
        if let Some(lim) = pag_obj.get("limit").and_then(|v| v.i64().ok()) {
            limit = Some(lim.clamp(1, 100) as u64);
        }
    }

    let query = DocumentQuery {
        filters,
        sort,
        fields: vec![],
        populate: PopulateSpec::All,
        locale: Some(locale),
        status: Some(status),
        search_query: None,
        page,
        page_size,
        start,
        limit,
    };

    services
        .content
        .find_many_public(schema, &query)
        .await
        .map_err(api_err_to_gql)
}

fn enforce_gql_perm(
    actor: &Actor,
    action: &str,
    target_uid: Option<&str>,
) -> Result<(), GqlError> {
    if actor.can(action, target_uid) {
        Ok(())
    } else {
        Err(GqlError::new(format!(
            "Forbidden: missing required permission '{action}'"
        )))
    }
}

fn extract_gql_status(
    ctx: &async_graphql::dynamic::ResolverContext<'_>,
    actor: &Actor,
) -> Result<DocumentStatus, GqlError> {
    let raw = ctx
        .args
        .get("status")
        .and_then(|v| v.string().ok())
        .unwrap_or("published");
    let status = if raw.eq_ignore_ascii_case("draft") {
        DocumentStatus::Draft
    } else {
        DocumentStatus::Published
    };
    if status == DocumentStatus::Draft && matches!(actor, Actor::Public { .. }) {
        return Err(GqlError::new(
            "Forbidden: authentication required to query draft documents",
        ));
    }
    Ok(status)
}

fn extract_gql_write_status(
    ctx: &async_graphql::dynamic::ResolverContext<'_>,
    default_status: DocumentStatus,
) -> Result<DocumentStatus, GqlError> {
    match ctx.args.get("status").and_then(|v| v.string().ok()) {
        Some(raw) if raw.eq_ignore_ascii_case("draft") => Ok(DocumentStatus::Draft),
        Some(raw) if raw.eq_ignore_ascii_case("published") => Ok(DocumentStatus::Published),
        Some(other) => Err(GqlError::new(format!(
            "Invalid status '{other}' (expected 'draft' or 'published')"
        ))),
        None => Ok(default_status),
    }
}

fn extract_gql_data_map(
    ctx: &async_graphql::dynamic::ResolverContext<'_>,
) -> Result<serde_json::Map<String, serde_json::Value>, GqlError> {
    let val = ctx
        .args
        .try_get("data")?
        .as_value()
        .clone()
        .into_json()
        .map_err(|e| GqlError::new(format!("Invalid data JSON: {e}")))?;
    val.as_object()
        .cloned()
        .ok_or_else(|| GqlError::new("Mutation 'data' argument must be a JSON object"))
}

fn actor_id_opt(actor: &Actor) -> Option<&str> {
    match actor {
        Actor::AdminUser { user_id, .. } | Actor::EndUser { user_id, .. } => Some(user_id.as_str()),
        Actor::AdminToken { token_id, .. } | Actor::ApiToken { token_id, .. } => {
            Some(token_id.as_str())
        }
        Actor::Public { .. } => None,
    }
}

fn api_err_to_gql(err: ApiError) -> GqlError {
    GqlError::new(err.to_string())
}

fn pascal_case(input: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if capitalize_next {
                out.push(ch.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                out.push(ch);
            }
        } else {
            capitalize_next = true;
        }
    }
    if out.is_empty() {
        "Entry".to_string()
    } else if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("T{out}")
    } else {
        out
    }
}
