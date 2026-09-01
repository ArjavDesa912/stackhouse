# 22 - Realtime 2.0

## 🔌 Bidirectional WebSocket Communication

### WebSocket vs SSE

```
┌─────────────────────────────────────────────────────────────┐
│           WEBSOCKET vs SSE COMPARISON                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  WebSocket (NEW):                                            │
│  ✅ Bidirectional (client can send messages)                 │
│  ✅ Lower latency (<10ms)                                    │
│  ✅ Binary support                                           │
│  ✅ Multiplexing (multiple subscriptions)                     │
│                                                              │
│  SSE (Legacy):                                               │
│  ✅ Unidirectional (server to client only)                   │
│  ✅ Browser support (all browsers)                           │
│  ✅ Simpler implementation                                   │
│  ⚠️ Higher latency                                          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### JavaScript Example

```javascript
// Connect
const ws = new WebSocket('ws://localhost:3000/v1/realtime');

ws.onopen = () => {
  console.log('Connected');

  // Subscribe to collection
  ws.send(JSON.stringify({
    type: 'Subscribe',
    key: 'users'
  }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'Data') {
    console.log('Update:', msg.key, msg.value);
  }
};

// Unsubscribe
ws.send(JSON.stringify({
  type: 'Unsubscribe',
  key: 'users'
}));
```

### Python Example

```python
import asyncio
import websockets
import json

async def stackhouse_client():
    uri = "ws://localhost:3000/v1/realtime"

    async with websockets.connect(uri) as ws:
        # Subscribe
        await ws.send(json.dumps({
            "type": "Subscribe",
            "key": "users"
        }))

        # Listen
        while True:
            msg = await ws.recv()
            data = json.loads(msg)
            print(f"Update: {data}")

asyncio.run(stackhouse_client())
```

---

**Next:** [API Reference](./50-API-Reference.md) 🚀
