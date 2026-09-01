const API_Base = import.meta.env.DEV ? 'http://localhost:3000' : window.location.origin;

function getToken() {
  return localStorage.getItem('stackhouse_access_token');
}

function getRefreshToken() {
  return localStorage.getItem('stackhouse_refresh_token');
}

function setTokens(access, refresh) {
  localStorage.setItem('stackhouse_access_token', access);
  if (refresh) localStorage.setItem('stackhouse_refresh_token', refresh);
}

function clearTokens() {
  localStorage.removeItem('stackhouse_access_token');
  localStorage.removeItem('stackhouse_refresh_token');
}

let isRefreshing = false;
let refreshPromise = null;

async function doRefresh() {
  const rt = getRefreshToken();
  if (!rt) throw new Error('No refresh token');
  const res = await fetch(`${API_Base}/v1/auth/refresh`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: rt }),
  });
  if (!res.ok) throw new Error('Refresh failed');
  const json = await res.json();
  if (!json.success) throw new Error(json.message || 'Refresh failed');
  setTokens(json.data.access_token, json.data.refresh_token);
  return json.data.access_token;
}

async function refreshAccessToken() {
  if (isRefreshing) return refreshPromise;
  isRefreshing = true;
  refreshPromise = doRefresh().finally(() => {
    isRefreshing = false;
    refreshPromise = null;
  });
  return refreshPromise;
}

export async function apiFetch(path, options = {}) {
  const url = path.startsWith('http') ? path : `${API_Base}${path}`;
  const token = getToken();

  const headers = {
    'Content-Type': 'application/json',
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(options.headers || {}),
  };

  if (options.body && typeof options.body !== 'string') {
    options.body = JSON.stringify(options.body);
  }

  let res = await fetch(url, { ...options, headers });

  if (res.status === 401 && getRefreshToken()) {
    try {
      await refreshAccessToken();
      headers.Authorization = `Bearer ${getToken()}`;
      res = await fetch(url, { ...options, headers });
    } catch {
      clearTokens();
      window.location.reload();
    }
  }

  return res;
}

export async function apiGet(path) {
  const res = await apiFetch(path, { method: 'GET' });
  const json = await res.json().catch(() => ({ success: false, message: 'Invalid JSON' }));
  return { ok: res.ok, status: res.status, data: json };
}

export async function apiPost(path, body) {
  const res = await apiFetch(path, { method: 'POST', body });
  const json = await res.json().catch(() => ({ success: false, message: 'Invalid JSON' }));
  return { ok: res.ok, status: res.status, data: json };
}

export async function apiPut(path, body) {
  const res = await apiFetch(path, { method: 'PUT', body });
  const json = await res.json().catch(() => ({ success: false, message: 'Invalid JSON' }));
  return { ok: res.ok, status: res.status, data: json };
}

export async function apiDelete(path) {
  const res = await apiFetch(path, { method: 'DELETE' });
  const json = await res.json().catch(() => ({ success: false, message: 'Invalid JSON' }));
  return { ok: res.ok, status: res.status, data: json };
}

export { API_Base, getToken, setTokens, clearTokens, getRefreshToken };
