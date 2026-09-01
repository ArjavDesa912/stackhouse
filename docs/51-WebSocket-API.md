# 51 - WebSocket API Protocol

## 🔌 Complete WebSocket Specification

### Connection

```
WS /v1/realtime
```

### Message Format

All messages are JSON:

```json
{
  "type": "MessageType",
  "key": "string",
  "value": {...},
  "seq": 0
}
```

### Message Types

#### 1. Subscribe

```json
{
  "type": "Subscribe",
  "key": "users"
}
```

#### 2. Unsubscribe

```json
{
  "type": "Unsubscribe",
  "key": "users"
}
```

#### 3. Data (Server → Client)

```json
{
  "type": "Data",
  "key": "users",
  "value": {
    "id": 1,
    "name": "Alice",
    "action": "insert"
  },
  "seq": 123
}
```

#### 4. Error

```json
{
  "type": "Error",
  "message": "Subscription failed"
}
```

#### 5. Ping/Pong

```json
// Client → Server
{"type": "Ping"}

// Server → Client
{"type": "Pong"}
```

### Best Practices

1. **Handle reconnection**
```javascript
ws.addEventListener('close', () => {
  setTimeout(() => {
    ws = new WebSocket(url);
  }, 1000);
});
```

2. **Resubscribe on reconnect**
```javascript
const subscriptions = ['users', 'documents'];
ws.onopen = () => {
  subscriptions.forEach(key => {
    ws.send(JSON.stringify({
      type: 'Subscribe',
      key
    }));
  });
};
```

3. **Error handling**
```javascript
ws.addEventListener('error', (error) => {
  console.error('WebSocket error:', error);
});
```

---

**Done!** 🎉
