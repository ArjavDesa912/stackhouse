import { createContext, useContext, useState, useEffect, useCallback } from 'react';
import { apiFetch, apiGet, apiPost, apiPut, setTokens, clearTokens, getToken, getRefreshToken } from '../lib/apiClient';

const AuthContext = createContext(null);

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}

export function AuthProvider({ children }) {
  const [user, setUser] = useState(null);
  const [isLoading, setIsLoading] = useState(true);

  const loadUser = useCallback(async () => {
    if (!getToken()) {
      setIsLoading(false);
      return;
    }
    try {
      const res = await apiGet('/v1/auth/me');
      if (res.ok && res.data.success) {
        setUser(res.data.data);
      } else {
        if (getRefreshToken()) {
          const refreshRes = await apiPost('/v1/auth/refresh', { refresh_token: getRefreshToken() });
          if (refreshRes.ok && refreshRes.data.success) {
            setTokens(refreshRes.data.data.access_token, refreshRes.data.data.refresh_token);
            const meRes = await apiGet('/v1/auth/me');
            if (meRes.ok && meRes.data.success) {
              setUser(meRes.data.data);
            } else {
              clearTokens();
              setUser(null);
            }
          } else {
            clearTokens();
            setUser(null);
          }
        } else {
          clearTokens();
          setUser(null);
        }
      }
    } catch {
      clearTokens();
      setUser(null);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadUser();
  }, [loadUser]);

  const login = async (email, password) => {
    const res = await apiPost('/v1/auth/login', { email, password });
    if (res.ok && res.data.success) {
      setTokens(res.data.data.access_token, res.data.data.refresh_token);
      setUser(res.data.data.user);
      return { success: true };
    }
    return { success: false, message: res.data?.message || 'Login failed' };
  };

  const signup = async (email, password, metadata = {}) => {
    const res = await apiPost('/v1/auth/signup', { email, password, metadata });
    if (res.ok && res.data.success) {
      setTokens(res.data.data.access_token, res.data.data.refresh_token);
      setUser(res.data.data.user);
      return { success: true };
    }
    return { success: false, message: res.data?.message || 'Signup failed' };
  };

  const logout = async () => {
    const rt = getRefreshToken();
    if (rt) {
      try { await apiPost('/v1/auth/logout', { refresh_token: rt }); } catch { /* ignore */ }
    }
    clearTokens();
    setUser(null);
    window.location.reload();
  };

  const refreshUser = async () => {
    const res = await apiGet('/v1/auth/me');
    if (res.ok && res.data.success) {
      setUser(res.data.data);
    }
  };

  const updateUser = async (updates) => {
    const res = await apiPut('/v1/auth/user', updates);
    if (res.ok && res.data.success) {
      setUser(res.data.data);
      return { success: true };
    }
    return { success: false, message: res.data?.message || 'Update failed' };
  };

  const isAdmin = () => {
    if (!user) return false;
    const meta = user.metadata || {};
    const role = (meta.role || '').toLowerCase();
    return role === 'admin' || role === 'owner' || meta.service_admin === true;
  };

  const value = {
    user,
    isLoading,
    isAuthenticated: !!user,
    login,
    signup,
    logout,
    refreshUser,
    updateUser,
    isAdmin,
  };

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  );
}
