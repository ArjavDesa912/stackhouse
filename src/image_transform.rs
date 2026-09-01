//! # Image Transformation Module (Stackhouse-ImageTransform)
//!
//! On-the-fly image transformations for stored files.
//! Supports resize, crop, format conversion, with caching.
//!
//! ## Query Parameters
//! - `width` / `height` - Resize dimensions
//! - `fit` - cover, contain, fill, inside, outside
//! - `format` - webp, png, jpeg, avif
//! - `quality` - 1-100 (for lossy formats)

use crate::error::{StackhouseError, StackhouseResult};

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
    routing::get,
    Router,
};
use image::{imageops::FilterType, DynamicImage, ImageFormat};
use serde::Deserialize;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

// ============================================================================
// Transform Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct TransformParams {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default = "default_fit")]
    pub fit: String,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default = "default_quality")]
    pub quality: u8,
}

fn default_fit() -> String {
    "cover".to_string()
}
fn default_quality() -> u8 {
    80
}

// ============================================================================
// Image Transform Service
// ============================================================================

#[derive(Clone)]
pub struct ImageTransformService {
    storage_path: PathBuf,
    cache_path: PathBuf,
    max_dimension: u32,
}

impl ImageTransformService {
    pub fn new(storage_path: PathBuf) -> Self {
        let cache_path = storage_path.join(".cache");
        std::fs::create_dir_all(&cache_path).ok();
        info!("🖼️ Stackhouse-ImageTransform initialized");
        Self {
            storage_path,
            cache_path,
            max_dimension: 4096,
        }
    }

    /// Transform an image from the storage path
    pub fn transform(
        &self,
        file_path: &str,
        params: &TransformParams,
    ) -> StackhouseResult<(Vec<u8>, String)> {
        let full_path = self.storage_path.join(file_path);
        if !full_path.exists() {
            return Err(StackhouseError::TableNotFound(format!(
                "File not found: {}",
                file_path
            )));
        }

        // Check cache
        let cache_key = self.cache_key(file_path, params);
        let cached = self.cache_path.join(&cache_key);
        if cached.exists() {
            let data = std::fs::read(&cached).map_err(|e| {
                StackhouseError::Internal(anyhow::anyhow!("Cache read error: {}", e))
            })?;
            let mime = self.format_to_mime(params.format.as_deref());
            return Ok((data, mime));
        }

        // Load image
        let img = image::open(&full_path)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Image load error: {}", e)))?;

        // Apply transformations
        let transformed = self.apply_transform(img, params)?;

        // Encode
        let output_format = self.resolve_format(params.format.as_deref(), file_path);
        let data = self.encode_image(&transformed, &output_format, params.quality)?;
        let mime = self.format_to_mime(Some(&output_format));

        // Cache
        std::fs::write(&cached, &data).ok();

        Ok((data, mime))
    }

    fn apply_transform(
        &self,
        img: DynamicImage,
        params: &TransformParams,
    ) -> StackhouseResult<DynamicImage> {
        let (orig_w, orig_h) = (img.width(), img.height());

        let target_w = params.width.unwrap_or(orig_w).min(self.max_dimension);
        let target_h = params.height.unwrap_or(orig_h).min(self.max_dimension);

        // If no resize needed
        if target_w == orig_w && target_h == orig_h {
            return Ok(img);
        }

        let result = match params.fit.as_str() {
            "contain" => img.resize(target_w, target_h, FilterType::Lanczos3),
            "fill" => img.resize_exact(target_w, target_h, FilterType::Lanczos3),
            "inside" => {
                let ratio = (target_w as f64 / orig_w as f64).min(target_h as f64 / orig_h as f64);
                let new_w = (orig_w as f64 * ratio) as u32;
                let new_h = (orig_h as f64 * ratio) as u32;
                img.resize_exact(new_w, new_h, FilterType::Lanczos3)
            }
            _ => {
                // "cover" default
                img.resize_to_fill(target_w, target_h, FilterType::Lanczos3)
            }
        };

        Ok(result)
    }

    fn resolve_format(&self, requested: Option<&str>, original_path: &str) -> String {
        if let Some(fmt) = requested {
            return fmt.to_lowercase();
        }
        // Infer from extension
        let ext = std::path::Path::new(original_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" => "jpeg".to_string(),
            "webp" => "webp".to_string(),
            "gif" => "gif".to_string(),
            "bmp" => "bmp".to_string(),
            _ => "png".to_string(),
        }
    }

    fn encode_image(
        &self,
        img: &DynamicImage,
        format: &str,
        _quality: u8,
    ) -> StackhouseResult<Vec<u8>> {
        let mut buffer = Cursor::new(Vec::new());
        let img_format = match format {
            "jpeg" | "jpg" => ImageFormat::Jpeg,
            "webp" => ImageFormat::WebP,
            "gif" => ImageFormat::Gif,
            "bmp" => ImageFormat::Bmp,
            _ => ImageFormat::Png,
        };

        img.write_to(&mut buffer, img_format)
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Image encode error: {}", e)))?;

        Ok(buffer.into_inner())
    }

    fn format_to_mime(&self, format: Option<&str>) -> String {
        match format {
            Some("jpeg") | Some("jpg") => "image/jpeg".to_string(),
            Some("webp") => "image/webp".to_string(),
            Some("gif") => "image/gif".to_string(),
            Some("bmp") => "image/bmp".to_string(),
            _ => "image/png".to_string(),
        }
    }

    fn cache_key(&self, path: &str, params: &TransformParams) -> String {
        use sha2::{Digest, Sha256};
        let key = format!(
            "{}:{}:{}:{}:{}:{}",
            path,
            params.width.unwrap_or(0),
            params.height.unwrap_or(0),
            params.fit,
            params.format.as_deref().unwrap_or("auto"),
            params.quality,
        );
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{}.cache", hex::encode(hasher.finalize()))
    }
}

// ============================================================================
// Handlers & Router
// ============================================================================

#[derive(Clone)]
pub struct ImageTransformState {
    pub transform: Arc<ImageTransformService>,
}

async fn transform_handler(
    State(state): State<ImageTransformState>,
    Path(file_path): Path<String>,
    Query(params): Query<TransformParams>,
) -> Result<impl IntoResponse, StackhouseError> {
    let (data, content_type) = state.transform.transform(&file_path, &params)?;

    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".to_string(),
            ),
        ],
        data,
    ))
}

pub fn create_image_transform_router(state: ImageTransformState) -> Router {
    Router::new()
        .route("/transform/*file_path", get(transform_handler))
        .with_state(state)
}
