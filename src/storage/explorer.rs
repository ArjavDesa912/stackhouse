//! # Stackhouse-Explorer
//!
//! Embedded dashboard for real-time data visualization.
//! Uses rust-embed to serve static assets from the binary.
//!
//! ## Key Features
//! - Auto-detection of data types for visualization
//! - Live streaming of data changes via SSE
//! - No configuration required

use axum::{
    body::Body,
    extract::Path,
    http::{header, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use rust_embed::RustEmbed;

/// Embedded UI assets from ./ui/dist
#[derive(RustEmbed)]
#[folder = "ui/dist"]
#[prefix = ""]
pub struct ExplorerAssets;

/// Creates the explorer router
pub fn create_explorer_router() -> Router {
    Router::new()
        .route("/explore", get(serve_index))
        .route("/explore/", get(serve_index))
        .route("/explore/*path", get(serve_static))
        .route("/assets/*path", get(serve_asset))
}

/// Serve the main index.html
async fn serve_index() -> impl IntoResponse {
    if ExplorerAssets::get("index.html").is_some() {
        serve_file("index.html").into_response()
    } else {
        axum::response::Html(fallback_explorer_html()).into_response()
    }
}

/// Serve static files
async fn serve_static(Path(path): Path<String>) -> impl IntoResponse {
    serve_file(&path)
}

/// Serve asset files
async fn serve_asset(Path(path): Path<String>) -> impl IntoResponse {
    serve_file(&format!("assets/{}", path))
}

/// Helper to serve embedded files
fn serve_file(path: &str) -> Response<Body> {
    match ExplorerAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => {
            // Try index.html for SPA routing
            if !path.contains('.') {
                if let Some(content) = ExplorerAssets::get("index.html") {
                    return Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/html")
                        .body(Body::from(content.data.into_owned()))
                        .unwrap();
                }
            }
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap()
        }
    }
}

/// Fallback HTML when no UI is built yet
pub fn fallback_explorer_html() -> &'static str {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>🛸 Stackhouse-Explorer</title>
    <style>
        :root {
            --bg: #0a0a0f;
            --card: #12121a;
            --border: #1f1f2e;
            --primary: #6366f1;
            --primary-glow: rgba(99, 102, 241, 0.2);
            --text: #e4e4e7;
            --text-muted: #71717a;
            --success: #10b981;
            --warning: #f59e0b;
            --error: #ef4444;
        }
        
        * { margin: 0; padding: 0; box-sizing: border-box; }
        
        body {
            font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
            background: var(--bg);
            color: var(--text);
            min-height: 100vh;
        }
        
        .container {
            max-width: 1400px;
            margin: 0 auto;
            padding: 2rem;
        }
        
        header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 3rem;
            padding-bottom: 1.5rem;
            border-bottom: 1px solid var(--border);
        }
        
        .logo {
            display: flex;
            align-items: center;
            gap: 0.75rem;
            font-size: 1.5rem;
            font-weight: 700;
        }
        
        .logo-icon {
            font-size: 2rem;
        }
        
        .status {
            display: flex;
            align-items: center;
            gap: 0.5rem;
            padding: 0.5rem 1rem;
            background: var(--card);
            border-radius: 9999px;
            font-size: 0.875rem;
        }
        
        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            background: var(--success);
            animation: pulse 2s infinite;
        }
        
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.5; }
        }
        
        .grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(350px, 1fr));
            gap: 1.5rem;
        }
        
        .card {
            background: var(--card);
            border: 1px solid var(--border);
            border-radius: 1rem;
            padding: 1.5rem;
            transition: all 0.3s ease;
        }
        
        .card:hover {
            border-color: var(--primary);
            box-shadow: 0 0 20px var(--primary-glow);
        }
        
        .card-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 1rem;
        }
        
        .card-title {
            font-size: 1.125rem;
            font-weight: 600;
        }
        
        .card-badge {
            padding: 0.25rem 0.75rem;
            background: var(--primary-glow);
            color: var(--primary);
            border-radius: 9999px;
            font-size: 0.75rem;
            font-weight: 500;
        }
        
        .stats {
            display: grid;
            grid-template-columns: repeat(2, 1fr);
            gap: 1rem;
            margin-bottom: 1rem;
        }
        
        .stat {
            padding: 1rem;
            background: var(--bg);
            border-radius: 0.5rem;
        }
        
        .stat-value {
            font-size: 1.5rem;
            font-weight: 700;
            color: var(--primary);
        }
        
        .stat-label {
            font-size: 0.75rem;
            color: var(--text-muted);
            margin-top: 0.25rem;
        }
        
        .columns-list {
            max-height: 200px;
            overflow-y: auto;
        }
        
        .column-item {
            display: flex;
            justify-content: space-between;
            padding: 0.5rem 0;
            border-bottom: 1px solid var(--border);
            font-size: 0.875rem;
        }
        
        .column-item:last-child {
            border-bottom: none;
        }
        
        .column-type {
            color: var(--text-muted);
            font-family: monospace;
        }
        
        .empty-state {
            text-align: center;
            padding: 4rem 2rem;
            color: var(--text-muted);
        }
        
        .empty-state h2 {
            font-size: 1.5rem;
            margin-bottom: 1rem;
            color: var(--text);
        }
        
        .code-block {
            background: var(--bg);
            border-radius: 0.5rem;
            padding: 1rem;
            font-family: 'Fira Code', monospace;
            font-size: 0.875rem;
            overflow-x: auto;
            margin-top: 1rem;
        }
        
        .code-block .comment { color: var(--text-muted); }
        .code-block .key { color: var(--primary); }
        .code-block .string { color: var(--success); }
        
        .btn {
            display: inline-flex;
            align-items: center;
            gap: 0.5rem;
            padding: 0.75rem 1.5rem;
            background: var(--primary);
            color: white;
            border: none;
            border-radius: 0.5rem;
            font-weight: 500;
            cursor: pointer;
            transition: all 0.2s;
        }
        
        .btn:hover {
            transform: translateY(-2px);
            box-shadow: 0 4px 12px var(--primary-glow);
        }
        
        #tables-container {
            min-height: 400px;
        }
        
        .loading {
            display: flex;
            justify-content: center;
            padding: 2rem;
        }
        
        .spinner {
            width: 40px;
            height: 40px;
            border: 3px solid var(--border);
            border-top-color: var(--primary);
            border-radius: 50%;
            animation: spin 1s linear infinite;
        }
        
        @keyframes spin {
            to { transform: rotate(360deg); }
        }
        
        .live-indicator {
            display: flex;
            align-items: center;
            gap: 0.5rem;
            font-size: 0.75rem;
            color: var(--success);
        }
        
        .live-dot {
            width: 6px;
            height: 6px;
            background: var(--success);
            border-radius: 50%;
            animation: pulse 1s infinite;
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div class="logo">
                <span class="logo-icon">🛸</span>
                <span>Stackhouse-Explorer</span>
            </div>
            <div class="status">
                <div class="status-dot"></div>
                <span>Connected</span>
            </div>
        </header>
        
        <div id="tables-container">
            <div class="loading">
                <div class="spinner"></div>
            </div>
        </div>
    </div>
    
    <script>
        const API_BASE = window.location.origin;
        
        async function fetchTables() {
            try {
                const response = await fetch(`${API_BASE}/v1/tables`);
                const data = await response.json();
                
                if (data.tables && data.tables.length > 0) {
                    renderTables(data.tables);
                } else {
                    renderEmptyState();
                }
            } catch (error) {
                console.error('Failed to fetch tables:', error);
                renderError(error);
            }
        }
        
        async function fetchTableStats(table) {
            try {
                const response = await fetch(`${API_BASE}/v1/tables/${table}`);
                const data = await response.json();
                return data.data;
            } catch (error) {
                console.error(`Failed to fetch stats for ${table}:`, error);
                return null;
            }
        }
        
        async function renderTables(tables) {
            const container = document.getElementById('tables-container');
            const statsPromises = tables.map(t => fetchTableStats(t));
            const allStats = await Promise.all(statsPromises);
            
            let html = '<div class="grid">';
            
            for (let i = 0; i < tables.length; i++) {
                const stats = allStats[i];
                if (!stats) continue;
                
                html += `
                    <div class="card" data-table="${stats.name}">
                        <div class="card-header">
                            <h3 class="card-title">${stats.name}</h3>
                            <span class="card-badge">Collection</span>
                        </div>
                        <div class="stats">
                            <div class="stat">
                                <div class="stat-value">${stats.row_count.toLocaleString()}</div>
                                <div class="stat-label">Documents</div>
                            </div>
                            <div class="stat">
                                <div class="stat-value">${stats.column_count}</div>
                                <div class="stat-label">Columns</div>
                            </div>
                        </div>
                        <div class="columns-list">
                            ${stats.columns.map(col => `
                                <div class="column-item">
                                    <span>${col.name}</span>
                                    <span class="column-type">${col.col_type || 'ANY'}</span>
                                </div>
                            `).join('')}
                        </div>
                        <div class="live-indicator" style="margin-top: 1rem;">
                            <div class="live-dot"></div>
                            <span>Live updates</span>
                        </div>
                    </div>
                `;
            }
            
            html += '</div>';
            container.innerHTML = html;
            
            // Set up SSE for each table
            tables.forEach(setupLiveUpdates);
        }
        
        function setupLiveUpdates(table) {
            const eventSource = new EventSource(`${API_BASE}/v1/stream/${table}`);
            
            eventSource.onmessage = (event) => {
                const data = JSON.parse(event.data);
                console.log(`[${table}] Event:`, data);
                
                if (data.event === 'insert' || data.event === 'batch_insert') {
                    // Refresh table stats
                    fetchTableStats(table).then(stats => {
                        if (stats) {
                            const card = document.querySelector(`[data-table="${table}"]`);
                            if (card) {
                                const rowCountEl = card.querySelector('.stat-value');
                                if (rowCountEl) {
                                    rowCountEl.textContent = stats.row_count.toLocaleString();
                                    rowCountEl.style.color = '#10b981';
                                    setTimeout(() => {
                                        rowCountEl.style.color = '';
                                    }, 1000);
                                }
                            }
                        }
                    });
                }
            };
            
            eventSource.onerror = () => {
                console.log(`[${table}] SSE connection lost, reconnecting...`);
            };
        }
        
        function renderEmptyState() {
            document.getElementById('tables-container').innerHTML = `
                <div class="empty-state">
                    <h2>🛸 No Collections Yet</h2>
                    <p>Start pushing data to create your first collection!</p>
                    <div class="code-block">
                        <span class="comment"># Push your first document</span><br>
                        curl -X POST ${API_BASE}/v1/push/users \\<br>
                        &nbsp;&nbsp;-H <span class="string">"Content-Type: application/json"</span> \\<br>
                        &nbsp;&nbsp;-d <span class="string">'{"name": "Alice", "email": "alice@stackhouse.db"}'</span>
                    </div>
                </div>
            `;
        }
        
        function renderError(error) {
            document.getElementById('tables-container').innerHTML = `
                <div class="empty-state">
                    <h2>⚠️ Connection Error</h2>
                    <p>${error.message}</p>
                    <button class="btn" onclick="fetchTables()">Retry</button>
                </div>
            `;
        }
        
        // Initial load
        fetchTables();
        
        // Refresh every 30s
        setInterval(fetchTables, 30000);
    </script>
</body>
</html>"#
}

/// Create a fallback explorer router when no UI is built
pub fn create_fallback_explorer_router() -> Router {
    Router::new()
        .route("/explore", get(serve_fallback))
        .route("/explore/", get(serve_fallback))
}

async fn serve_fallback() -> impl IntoResponse {
    axum::response::Html(fallback_explorer_html())
}
