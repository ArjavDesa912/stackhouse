# 21 - JavaScript Functions

## ⚡ Serverless Compute with JavaScript

```
┌─────────────────────────────────────────────────────────────┐
│     ╔══════════════════════════════════════════════════╗   │
│     ║                                                  ║   │
│     ║    Run Custom Logic Safely at the Edge          ║   │
│     ║                                                  ║   │
│     ╚══════════════════════════════════════════════════╝   │
└─────────────────────────────────────────────────────────────┘
```

## What are JavaScript Functions?

> The functions router is live under `/v1/functions` in `main.rs`. The examples below are real curl commands against a running server.

Stackhouse functions are written in JavaScript and executed by the embedded Boa engine. The runtime resolves a handler in one of three forms:
- a global `handler` function
- `exports.handler`
- `module.exports`

The function receives a single `input` argument (a JSON value) and should return a JSON-serializable value.

`runtime` accepts `javascript`, `typescript`, `wasm_rust`, or `wasm_js` in the request, but every value executes as raw JavaScript via Boa today — the `wasm_rust`/`wasm_js` values are recorded for forward compatibility only and do not trigger any WASM compilation or execution.

## Quick Example

### 1. Write a JavaScript function

```javascript
// process.js
exports.handler = function(input) {
    return { doubled: input.value * 2 };
};
```

### 2. Deploy to Stackhouse

```bash
curl -X POST http://localhost:3000/v1/functions/deploy \
  -H "Content-Type: application/json" \
  -d '{
    "name": "double",
    "runtime": "javascript",
    "source_code": "exports.handler = (input) => ({ doubled: input.value * 2 });"
  }'
```

### 3. Execute

```bash
curl -X POST http://localhost:3000/v1/functions/invoke/double \
  -H "Content-Type: application/json" \
  -d '{"value": 21}'

# Returns: {"success": true, "output": {"doubled": 42}}
```

## Use Cases

### 1. Data Validation

```javascript
exports.handler = (input) => {
    if (!input.email || !input.email.includes("@")) {
        throw new Error("Invalid email");
    }
    return { valid: true };
};
```

### 2. Data Transformation

```javascript
exports.handler = (input) => {
    return {
        ...input,
        total: input.price * input.quantity
    };
};
```

### 3. Business Logic

```javascript
exports.handler = (input) => {
    if (input.amount > 1000) {
        return input.amount * 0.9;
    }
    return input.amount;
};
```

## API Reference

### Deploy Function

```http
POST /v1/functions/deploy
Content-Type: application/json

{
  "name": "myfunc",
  "runtime": "javascript",
  "entrypoint": "handler",
  "source_code": "exports.handler = (input) => input"
}
```

`runtime` accepts `javascript`, `typescript`, `wasm_rust`, or `wasm_js` — all four are run by the Boa JS engine today; only the value is stored differently. `entrypoint` defaults to `handler`.

### Execute Function

```http
POST /v1/functions/invoke/:name
Content-Type: application/json

{
  "input": { "value": 42 }
}
```

### List Functions

```http
GET /v1/functions
```

### Delete Function

```http
DELETE /v1/functions/:id
```

## Security

```
┌─────────────────────────────────────────────────────────────┐
│              JS SANDBOX SECURITY                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ✅ Memory Isolation      → Boa engine isolates execution    │
│  ✅ Resource Limits       → CPU time, memory caps           │
│  ✅ No File System        → Cannot read/write files          │
│  ✅ Timeout Enforcement   → Prevent infinite loops          │
│  ✅ Fast Execution        → Compiled JS in the same process  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Best Practices

1. **Keep functions small** - source code is stored in the database
2. **Use timeouts** - default 30s; prevent infinite loops
3. **Limit memory** - default 128MB
4. **Handle errors** - Return clear error messages
5. **Return JSON-serializable values** - Boa serializes the result to JSON

## Resources

- [Performance Guide](./41-Performance.md)
- [Realtime](./22-Realtime.md)

---

**Ready to deploy functions?** Continue to [Realtime](./22-Realtime.md) 🚀
