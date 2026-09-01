# Stackhouse Python SDK

Async Python client for the Stackhouse BaaS platform.

## Install

```bash
pip install stackhouse
```

Or install from source:

```bash
cd stackhouse/sdks/python
pip install -e .
```

## Quick Start

```python
import asyncio
from stackhouse import StackhouseClient

async def main():
    async with StackhouseClient("http://localhost:3000", api_key="vdb_...") as client:
        # Insert data (schema-later: the table is created automatically)
        result = await client.from_table("posts").insert({
            "title": "Hello from Python",
            "published": True,
        })
        print("inserted:", result)

        # Query
        rows = await client.from_table("posts").eq("published", True).limit(10).execute()
        print("rows:", rows)

        # SQL query
        rows = await client.query("SELECT * FROM stackhouse_posts WHERE data->>'published' = 'true'")
        print("sql rows:", rows)

        # Auth
        session = await client.auth.sign_in("user@example.com", "password")

        # Storage
        await client.storage.create_bucket("documents")

        # Realtime (WebSocket)
        await client.realtime.subscribe("posts", lambda event: print(event))

asyncio.run(main())
```

## Features

- `StackhouseClient` — async context-manager client with API-key auth.
- `QueryBuilder` — fluent `SELECT / INSERT / UPDATE / DELETE` against collections.
- `AuthClient` — sign up, sign in, refresh, MFA.
- `StorageClient` — bucket upload, download, signed URLs.
- `VectorClient` — collection search and upsert.
- `RealtimeClient` — WebSocket pub/sub over collections.

## Development

```bash
pip install -e ".[dev]"
pytest
```
