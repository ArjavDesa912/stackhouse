"""Realtime client for Stackhouse Python SDK."""

import asyncio
import json
from typing import Callable, Dict, Optional

try:
    import websockets
except ImportError:
    websockets = None


class RealtimeClient:
    """WebSocket-based realtime subscriptions."""

    def __init__(self, url: str, api_key: str):
        self._url = url.replace("http", "ws") + "/v1/realtime"
        self._api_key = api_key
        self._ws = None
        self._subscriptions: Dict[str, Callable] = {}
        self._running = False

    async def connect(self):
        if websockets is None:
            raise ImportError("Install websockets: pip install websockets")
        self._ws = await websockets.connect(
            self._url,
            extra_headers={"Authorization": f"Bearer {self._api_key}"},
        )
        self._running = True
        asyncio.create_task(self._listen())

    async def _listen(self):
        try:
            async for message in self._ws:
                data = json.loads(message)
                channel = data.get("channel", "")
                if channel in self._subscriptions:
                    self._subscriptions[channel](data)
        except Exception:
            self._running = False

    def on(self, channel: str, callback: Callable):
        """Subscribe to a channel."""
        self._subscriptions[channel] = callback
        if self._ws:
            asyncio.create_task(self._ws.send(json.dumps({
                "type": "subscribe",
                "channel": channel,
            })))
        return self

    async def send(self, channel: str, event: str, payload: Dict):
        """Send a message to a channel."""
        if self._ws:
            await self._ws.send(json.dumps({
                "type": "broadcast",
                "channel": channel,
                "event": event,
                "payload": payload,
            }))

    async def unsubscribe(self, channel: str):
        self._subscriptions.pop(channel, None)
        if self._ws:
            await self._ws.send(json.dumps({
                "type": "unsubscribe",
                "channel": channel,
            }))

    async def close(self):
        self._running = False
        if self._ws:
            await self._ws.close()
