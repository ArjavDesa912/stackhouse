#!/usr/bin/env python3
"""
Comprehensive Features API Test for Stackhouse
This tests all the newly wired modules: Auth (MFA, Magic Link, Phone OTP, Captcha), 
Storage, RLS, Vectors, Realtime, Connections, Admin (Backups, Extensions, Network, Logs, Branching, Teams).
"""

import requests
import json
import time
import uuid

BASE_URL = "http://localhost:3000"
headers = {"Content-Type": "application/json"}
auth_token = None

def setup_auth():
    global auth_token
    print("  Setting up Authentication...")
    
    login_data = {
        "email": "test_admin@stackhouse.local",
        "password": "supersecretpassword"
    }
    response = requests.post(f"{BASE_URL}/v1/auth/signup", json=login_data)
    if response.status_code != 201:
        response = requests.post(f"{BASE_URL}/v1/auth/login", json=login_data)
        
    assert response.status_code in [200, 201], f"Auth failed: {response.text}"
    auth_token = response.json().get("data", {}).get("access_token")
    headers["Authorization"] = f"Bearer {auth_token}"
    print("    [OK] Authentication successful.")

# ============================================
# NEW FEATURE TESTS
# ============================================

def test_health():
    response = requests.get(f"{BASE_URL}/health")
    print("Health:", response.status_code)

def test_db_crud():
    print("  Testing Database CRUD API...")
    # Create Table / Push Data
    collection = "test_users_data"
    res = requests.post(f"{BASE_URL}/v1/push/{collection}", json={"name": "Alice", "age": 30}, headers=headers)
    print(f"    POST /v1/push/{collection}: {res.status_code}")
    
    if res.status_code == 201:
        doc_id = res.json().get("data", {}).get("id")
        # Query Data
        res = requests.get(f"{BASE_URL}/v1/query/{collection}", headers=headers)
        print(f"    GET /v1/query/{collection}: {res.status_code}")
        
        # Get by ID
        if doc_id:
            res = requests.get(f"{BASE_URL}/v1/query/{collection}/{doc_id}", headers=headers)
            print(f"    GET /v1/query/{collection}/{doc_id}: {res.status_code}")
            
            # Update Document
            res = requests.post(f"{BASE_URL}/v1/update/{collection}/{doc_id}", json={"age": 31}, headers=headers)
            print(f"    POST /v1/update/{collection}/{doc_id}: {res.status_code}")
            
            # Delete Document
            res = requests.post(f"{BASE_URL}/v1/delete/{collection}/{doc_id}", headers=headers)
            print(f"    POST /v1/delete/{collection}/{doc_id}: {res.status_code}")

def test_sql():
    print("  Testing SQL API...")
    res = requests.post(f"{BASE_URL}/v1/sql/query", json={"query": "SELECT 1 as num"}, headers=headers)
    print(f"    POST /v1/sql/query: {res.status_code}")
    res = requests.post(f"{BASE_URL}/v1/sql/execute", json={"query": "CREATE TABLE IF NOT EXISTS sql_test_table (id SERIAL PRIMARY KEY)"}, headers=headers)
    print(f"    POST /v1/sql/execute: {res.status_code}")

def test_teams():
    print("  Testing Teams API...")
    # List teams
    res = requests.get(f"{BASE_URL}/v1/teams", headers=headers)
    print(f"    GET /teams: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.post(f"{BASE_URL}/v1/teams", json={"name": f"Test Team {uuid.uuid4()}"}, headers=headers)
    print(f"    POST /teams: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_phone_otp():
    print("  Testing Phone OTP API...")
    res = requests.post(f"{BASE_URL}/v1/auth/phone/send", json={"phone": "+1234567890"}, headers=headers)
    print(f"    POST /phone/send: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_mfa():
    print("  Testing MFA API...")
    res = requests.post(f"{BASE_URL}/v1/auth/mfa/enroll", headers=headers)
    print(f"    POST /mfa/enroll: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.get(f"{BASE_URL}/v1/auth/mfa/status", headers=headers)
    print(f"    GET /mfa/status: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_magic_link():
    print("  Testing Magic Link API...")
    res = requests.post(f"{BASE_URL}/v1/auth/magic-link", json={"email": "test@example.com", "redirect_to": "http://localhost:3000"}, headers=headers)
    print(f"    POST /magic-link: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_captcha():
    print("  Testing Captcha API...")
    res = requests.get(f"{BASE_URL}/v1/auth/captcha", headers=headers)
    print(f"    GET /captcha: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_storage():
    print("  Testing Storage API...")
    res = requests.get(f"{BASE_URL}/v1/storage/buckets", headers=headers)
    print(f"    GET /buckets: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.post(f"{BASE_URL}/v1/storage/buckets", json={"name": "test-bucket2", "public": True}, headers=headers)
    print(f"    POST /buckets: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.get(f"{BASE_URL}/v1/storage/list/test-bucket2", headers=headers)
    print(f"    GET /list/test-bucket: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_image_transform():
    print("  Testing Image Transform API...")
    res = requests.get(f"{BASE_URL}/v1/storage/transform/test.jpg?width=100", headers=headers)
    print(f"    GET /transform: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_rls():
    print("  Testing RLS API...")
    res = requests.get(f"{BASE_URL}/v1/rls/test_users_data/status", headers=headers)
    print(f"    GET /rls/status: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.post(f"{BASE_URL}/v1/rls/test_users_data/enable", headers=headers)
    print(f"    POST /rls/enable: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_realtime_presence_broadcast():
    print("  Testing Realtime Presence & Broadcast...")
    res = requests.get(f"{BASE_URL}/v1/realtime/presence", headers=headers)
    print(f"    GET /presence: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.post(f"{BASE_URL}/v1/realtime/broadcast/send", json={"event": "test", "payload": {}, "channel": "test_broadcast_chan"}, headers=headers)
    print(f"    POST /broadcast: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_admin_extensions_branching_network():
    print("  Testing Admin Extensions, Branching, Network...")
    res = requests.get(f"{BASE_URL}/v1/admin/extensions", headers=headers)
    print(f"    GET /extensions: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.get(f"{BASE_URL}/v1/admin/branches", headers=headers)
    print(f"    GET /branches: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.get(f"{BASE_URL}/v1/admin/network/rules", headers=headers)
    print(f"    GET /network/rules: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.post(f"{BASE_URL}/v1/admin/network/enable", headers=headers)
    print(f"    POST /network/enable: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_backup_logs():
    print("  Testing Backups & Logs...")
    res = requests.get(f"{BASE_URL}/v1/admin/backups", headers=headers)
    print(f"    GET /backups: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.get(f"{BASE_URL}/v1/admin/logs/drains", headers=headers)
    print(f"    GET /logs/drains: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_graphql():
    print("  Testing GraphQL...")
    # Test playground load
    res = requests.get(f"{BASE_URL}/v1/graphql/playground", headers=headers)
    print(f"    GET /graphql/playground: {res.status_code} {res.text if res.status_code != 200 else ''}")
    res = requests.post(f"{BASE_URL}/v1/graphql", json={"query": "{ hello }"}, headers=headers)
    print(f"    POST /graphql: {res.status_code} {res.text if res.status_code != 200 else ''}")

def test_metrics():
    print("  Testing Metrics...")
    res = requests.get(f"{BASE_URL}/v1/metrics/summary", headers=headers)
    print(f"    GET /metrics/summary: {res.status_code} {res.text if res.status_code != 200 else ''}")


def main():
    print("=" * 60)
    print("STACKHOUSE COMPREHENSIVE NEW API TESTS")
    print("=" * 60)
    
    test_health()
    setup_auth()
    
    test_db_crud()
    test_sql()
    
    test_teams()
    test_phone_otp()
    test_mfa()
    test_magic_link()
    test_captcha()
    test_storage()
    test_image_transform()
    test_rls()
    test_realtime_presence_broadcast()
    test_admin_extensions_branching_network()
    test_backup_logs()
    test_graphql()
    test_metrics()
    
    print("\n[SUCCESS] TEST SCRIPT COMPLETED")

if __name__ == "__main__":
    main()
