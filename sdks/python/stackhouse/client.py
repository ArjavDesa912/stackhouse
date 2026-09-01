"""Core Stackhouse client for Python."""

import httpx
from typing import Optional, Dict, Any, List


class StackhouseClient:
    """Main entry point for Stackhouse Python SDK."""

    def __init__(self, url: str, api_key: str, timeout: float = 30.0):
        self.url = url.rstrip("/")
        self.api_key = api_key
        self._client = httpx.AsyncClient(
            base_url=self.url,
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
                "X-Client-Info": "stackhouse-python/0.1.0",
            },
            timeout=timeout,
        )

    @property
    def auth(self):
        from .auth import AuthClient
        return AuthClient(self._client)

    @property
    def storage(self):
        from .storage import StorageClient
        return StorageClient(self._client)

    @property
    def vectors(self):
        from .vectors import VectorClient
        return VectorClient(self._client)

    @property
    def realtime(self):
        from .realtime import RealtimeClient
        return RealtimeClient(self.url, self.api_key)

    # --- Database Operations ---

    async def query(self, sql: str, params: Optional[List] = None) -> Dict[str, Any]:
        """Execute a read-only SQL query."""
        resp = await self._client.post("/v1/sql/query", json={"sql": sql, "params": params or []})
        resp.raise_for_status()
        return resp.json()

    async def execute_sql(self, sql: str, params: Optional[List] = None) -> Dict[str, Any]:
        """Execute a write SQL statement (requires admin scope)."""
        resp = await self._client.post("/v1/sql/execute", json={"sql": sql, "params": params or []})
        resp.raise_for_status()
        return resp.json()

    def from_table(self, table: str) -> "QueryBuilder":
        """Start a query builder for a table."""
        return QueryBuilder(self._client, table)

    async def rpc(self, function_name: str, params: Optional[Dict] = None) -> Dict[str, Any]:
        """Call a database function."""
        resp = await self._client.post(f"/v1/rpc/{function_name}", json=params or {})
        resp.raise_for_status()
        return resp.json()

    # --- AI Operations ---

    async def embed(self, text: str, model: str = "nomic-embed-text") -> List[float]:
        """Generate an embedding."""
        resp = await self._client.post("/v1/ai/embed", json={"text": text, "model": model})
        resp.raise_for_status()
        return resp.json()["embedding"]

    async def chat(self, messages: List[Dict], model: str = "gpt-4o-mini", **kwargs) -> Dict:
        """Send a chat completion request."""
        body = {"messages": messages, "model": model, **kwargs}
        resp = await self._client.post("/v1/ai/chat", json=body)
        resp.raise_for_status()
        return resp.json()

    async def invoke_agent(self, agent_id: str, input: Dict) -> Dict:
        """Invoke an AI agent."""
        resp = await self._client.post(f"/v1/ai/agents/{agent_id}/invoke", json=input)
        resp.raise_for_status()
        return resp.json()

    # --- Functions ---

    async def invoke_function(self, name: str, payload: Optional[Dict] = None) -> Dict:
        """Invoke an edge function."""
        resp = await self._client.post(f"/v1/functions/invoke/{name}", json={"input": payload or {}})
        resp.raise_for_status()
        return resp.json()

    async def close(self):
        await self._client.aclose()

    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        await self.close()


class QueryBuilder:
    """Fluent query builder for table operations."""

    def __init__(self, client: httpx.AsyncClient, table: str):
        self._client = client
        self._table = table
        self._filters: List[Dict] = []
        self._select_cols = "*"
        self._order_by: Optional[str] = None
        self._limit_val: Optional[int] = None

    def select(self, columns: str = "*"):
        self._select_cols = columns
        return self

    def eq(self, column: str, value: Any):
        self._filters.append({"column": column, "op": "eq", "value": value})
        return self

    def gt(self, column: str, value: Any):
        self._filters.append({"column": column, "op": "gt", "value": value})
        return self

    def lt(self, column: str, value: Any):
        self._filters.append({"column": column, "op": "lt", "value": value})
        return self

    def like(self, column: str, pattern: str):
        self._filters.append({"column": column, "op": "like", "value": pattern})
        return self

    def order(self, column: str, ascending: bool = True):
        self._order_by = f"{column}:{'asc' if ascending else 'desc'}"
        return self

    def limit(self, count: int):
        self._limit_val = count
        return self

    async def execute(self) -> Dict:
        """Run the SELECT query."""
        q: Dict[str, Any] = {"limit": str(self._limit_val) if self._limit_val else "100"}
        if self._order_by:
            q["order_by"] = self._order_by
        for f in self._filters:
            q[f["column"]] = str(f["value"])
        resp = await self._client.get(f"/v1/query/{self._table}", params=q)
        resp.raise_for_status()
        return resp.json()

    async def insert(self, data: Dict) -> Dict:
        resp = await self._client.post(f"/v1/push/{self._table}", json=data)
        resp.raise_for_status()
        return resp.json()

    async def update(self, data: Dict) -> Dict:
        resp = await self._client.post(f"/v1/update/{self._table}", json={
            "data": data,
            "filters": {f["column"]: f["value"] for f in self._filters},
        })
        resp.raise_for_status()
        return resp.json()

    async def delete(self) -> Dict:
        resp = await self._client.post(f"/v1/delete/{self._table}", json={
            "filters": {f["column"]: f["value"] for f in self._filters},
        })
        resp.raise_for_status()
        return resp.json()
