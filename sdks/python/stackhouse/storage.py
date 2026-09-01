"""Storage client for Stackhouse Python SDK."""

import httpx
from typing import Optional, Dict, Any, BinaryIO


class StorageClient:
    """File storage operations."""

    def __init__(self, client: httpx.AsyncClient):
        self._client = client

    def bucket(self, name: str) -> "BucketClient":
        return BucketClient(self._client, name)

    async def list_buckets(self) -> Dict:
        resp = await self._client.get("/v1/storage/buckets")
        resp.raise_for_status()
        return resp.json()

    async def create_bucket(self, name: str, public: bool = False) -> Dict:
        resp = await self._client.post("/v1/storage/buckets", json={
            "name": name,
            "public": public,
        })
        resp.raise_for_status()
        return resp.json()


class BucketClient:
    """Operations on a specific bucket."""

    def __init__(self, client: httpx.AsyncClient, bucket: str):
        self._client = client
        self._bucket = bucket

    async def upload(self, path: str, data: bytes, content_type: str = "application/octet-stream") -> Dict:
        resp = await self._client.post(
            f"/v1/storage/{self._bucket}/upload",
            files={"file": (path, data, content_type)},
        )
        resp.raise_for_status()
        return resp.json()

    async def download(self, path: str) -> bytes:
        resp = await self._client.get(f"/v1/storage/{self._bucket}/{path}")
        resp.raise_for_status()
        return resp.content

    async def list(self, prefix: str = "", limit: int = 100) -> Dict:
        resp = await self._client.get(f"/v1/storage/{self._bucket}", params={
            "prefix": prefix,
            "limit": limit,
        })
        resp.raise_for_status()
        return resp.json()

    async def delete(self, paths: list) -> Dict:
        resp = await self._client.post(f"/v1/storage/{self._bucket}/delete", json={"paths": paths})
        resp.raise_for_status()
        return resp.json()

    async def get_signed_url(self, path: str, expires_in: int = 3600) -> str:
        resp = await self._client.post(f"/v1/storage/{self._bucket}/sign", json={
            "path": path,
            "expires_in": expires_in,
        })
        resp.raise_for_status()
        return resp.json()["url"]

    async def move(self, from_path: str, to_path: str) -> Dict:
        resp = await self._client.post(f"/v1/storage/{self._bucket}/move", json={
            "from": from_path,
            "to": to_path,
        })
        resp.raise_for_status()
        return resp.json()
