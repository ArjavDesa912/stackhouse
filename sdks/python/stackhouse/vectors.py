"""Vector database client for Stackhouse Python SDK."""

import httpx
from typing import Optional, Dict, Any, List


class VectorClient:
    """Vector/embedding operations."""

    def __init__(self, client: httpx.AsyncClient):
        self._client = client

    async def create_collection(self, name: str, dimension: int = 384, distance: str = "cosine") -> Dict:
        resp = await self._client.post("/v1/vectors/collections", json={
            "name": name,
            "dimension": dimension,
            "distance": distance,
        })
        resp.raise_for_status()
        return resp.json()

    async def upsert(self, collection: str, documents: List[Dict]) -> Dict:
        """Upsert documents with auto-embedding."""
        resp = await self._client.post(f"/v1/vectors/{collection}/upsert", json={
            "documents": documents,
        })
        resp.raise_for_status()
        return resp.json()

    async def search(self, collection: str, query: str, top_k: int = 10,
                     filter: Optional[Dict] = None, hybrid: bool = False) -> Dict:
        """Semantic search (optionally hybrid with full-text)."""
        body = {
            "query": query,
            "top_k": top_k,
            "hybrid": hybrid,
        }
        if filter:
            body["filter"] = filter
        resp = await self._client.post(f"/v1/vectors/{collection}/search", json=body)
        resp.raise_for_status()
        return resp.json()

    async def delete(self, collection: str, ids: List[str]) -> Dict:
        resp = await self._client.post(f"/v1/vectors/{collection}/delete", json={"ids": ids})
        resp.raise_for_status()
        return resp.json()

    async def get(self, collection: str, ids: List[str]) -> Dict:
        resp = await self._client.post(f"/v1/vectors/{collection}/get", json={"ids": ids})
        resp.raise_for_status()
        return resp.json()

    async def list_collections(self) -> Dict:
        resp = await self._client.get("/v1/vectors/collections")
        resp.raise_for_status()
        return resp.json()
