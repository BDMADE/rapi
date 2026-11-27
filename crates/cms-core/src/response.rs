use serde::{Deserialize, Serialize};

/// Strapi v5-compatible response wrapper (`{ data, meta }`).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApiResponse<T> {
    pub data: T,
    pub meta: ResponseMeta,
}

impl<T> ApiResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            meta: ResponseMeta::default(),
        }
    }

    pub fn with_pagination(data: T, pagination: PaginationMeta) -> Self {
        Self {
            data,
            meta: ResponseMeta {
                pagination: Some(pagination),
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResponseMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PaginationMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    pub page: u64,
    pub page_size: u64,
    pub page_count: u64,
    pub total: u64,
}

impl PaginationMeta {
    pub fn compute(page: u64, page_size: u64, total: u64) -> Self {
        let safe_size = page_size.max(1);
        let page_count = if total == 0 {
            0
        } else {
            total.div_ceil(safe_size)
        };
        Self {
            page: page.max(1),
            page_size: safe_size,
            page_count,
            total,
        }
    }
}
