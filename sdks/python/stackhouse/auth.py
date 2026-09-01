"""Auth client for Stackhouse Python SDK."""

import httpx
from typing import Optional, Dict, Any


class AuthClient:
    """Authentication operations."""

    def __init__(self, client: httpx.AsyncClient):
        self._client = client

    async def sign_up(self, email: str, password: str, metadata: Optional[Dict] = None) -> Dict:
        resp = await self._client.post("/v1/auth/signup", json={
            "email": email,
            "password": password,
            "metadata": metadata or {},
        })
        resp.raise_for_status()
        return resp.json()

    async def sign_in(self, email: str, password: str) -> Dict:
        resp = await self._client.post("/v1/auth/login", json={
            "email": email,
            "password": password,
        })
        resp.raise_for_status()
        return resp.json()

    async def sign_in_with_oauth(self, provider: str, redirect_url: str) -> Dict:
        resp = await self._client.post("/v1/auth/oauth", json={
            "provider": provider,
            "redirect_url": redirect_url,
        })
        resp.raise_for_status()
        return resp.json()

    async def sign_out(self, token: str) -> Dict:
        resp = await self._client.post("/v1/auth/logout", headers={"Authorization": f"Bearer {token}"})
        resp.raise_for_status()
        return resp.json()

    async def get_user(self, token: str) -> Dict:
        resp = await self._client.get("/v1/auth/me", headers={"Authorization": f"Bearer {token}"})
        resp.raise_for_status()
        return resp.json()

    async def refresh_token(self, refresh_token: str) -> Dict:
        resp = await self._client.post("/v1/auth/refresh", json={"refresh_token": refresh_token})
        resp.raise_for_status()
        return resp.json()

    async def reset_password(self, email: str) -> Dict:
        resp = await self._client.post("/v1/auth/reset-password", json={"email": email})
        resp.raise_for_status()
        return resp.json()

    async def verify_mfa(self, token: str, code: str) -> Dict:
        resp = await self._client.post("/v1/auth/mfa/verify", json={"token": token, "code": code})
        resp.raise_for_status()
        return resp.json()
