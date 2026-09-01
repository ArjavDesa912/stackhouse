# 31 - Row-Level Security

## 🔒 Fine-Grained Access Control with RLS

### What is RLS?

Row-Level Security (RLS) allows you to control which rows users can access based on policies.

### Creating Policies

```sql
-- Example: Users can only see their own data
CREATE POLICY user_data_policy ON users
FOR SELECT
USING (user_id = auth.uid());

-- Example: Admins can see everything
CREATE POLICY admin_policy ON users
FOR ALL
USING (auth.role = 'admin');
```

### Policy Evaluation

```
┌─────────────────────────────────────────────────────────────┐
│              POLICY EVALUATION FLOW                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Request arrives                                            │
│      ↓                                                      │
│  Extract user info from JWT                                 │
│      ↓                                                      │
│  Check applicable policies                                   │
│      ↓                                                      │
│  Filter rows based on policy rules                          │
│      ↓                                                      │
│  Return only authorized rows                                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Example Policies

```javascript
// Policy 1: Users see own data
{
  "name": "user_isolation",
  "table": "documents",
  "action": "SELECT",
  "condition": "owner_id = auth.uid()"
}

// Policy 2: Public documents visible to all
{
  "name": "public_docs",
  "table": "documents",
  "action": "SELECT",
  "condition": "is_public = true"
}

// Policy 3: Editors can modify
{
  "name": "editor_access",
  "table": "documents",
  "action": "UPDATE",
  "condition": "auth.role IN ('editor', 'admin')"
}
```

### Best Practices

1. **Start restrictive** - Deny all, then allow specific
2. **Test policies** - Use test mode to verify
3. **Log denials** - Monitor unauthorized access attempts
4. **Keep it simple** - Complex policies are hard to debug

---

**Next:** [Storage](./32-Storage.md)
