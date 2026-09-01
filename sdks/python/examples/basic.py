import asyncio
import os
from stackhouse import StackhouseClient


async def main():
    url = os.environ.get("STACKHOUSE_URL", "http://localhost:3000")
    api_key = os.environ["STACKHOUSE_API_KEY"]

    async with StackhouseClient(url, api_key) as client:
        # Insert some records (schema-later)
        await client.from_table("tasks").insert({
            "title": "Build Python SDK",
            "done": False,
            "tags": ["sdk", "python"],
        })

        await client.from_table("tasks").insert({
            "title": "Ship v1",
            "done": True,
            "tags": ["launch"],
        })

        # Query with filters
        rows = await client.from_table("tasks").eq("done", False).limit(10).execute()
        print("Pending tasks:", rows)

        # Raw SQL
        rows = await client.query(
            "SELECT id, data FROM stackhouse_tasks WHERE data->>'done' = 'false'"
        )
        print("SQL pending:", rows)


if __name__ == "__main__":
    asyncio.run(main())
