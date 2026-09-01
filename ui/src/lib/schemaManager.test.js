import { describe, expect, it } from 'vitest'

import {
  buildAddColumnSql,
  buildCreateTableSql,
  canEditTableRows,
  confirmTableDeletion,
  inferTableRelationships,
  normalizeImportRows,
} from './schemaManager'

describe('buildCreateTableSql', () => {
  it('builds create table sql with primary key and constraints', () => {
    expect(
      buildCreateTableSql('accounts', [
        { name: 'id', type: 'INTEGER', pk: true, notNull: true },
        { name: 'name', type: 'TEXT', pk: false, notNull: true },
      ]),
    ).toBe('CREATE TABLE accounts (id INTEGER PRIMARY KEY, name TEXT NOT NULL)')
  })
})

describe('buildAddColumnSql', () => {
  it('builds alter table add column sql', () => {
    expect(
      buildAddColumnSql('accounts', {
        name: 'status',
        type: 'TEXT',
        notNull: false,
      }),
    ).toBe('ALTER TABLE accounts ADD COLUMN status TEXT')
  })

  it('includes not-null constraints when requested', () => {
    expect(
      buildAddColumnSql('accounts', {
        name: 'plan_id',
        type: 'BIGINT',
        notNull: true,
      }),
    ).toBe('ALTER TABLE accounts ADD COLUMN plan_id BIGINT NOT NULL')
  })
})

describe('confirmTableDeletion', () => {
  it('requires an exact typed confirmation', () => {
    expect(confirmTableDeletion('accounts', 'accounts')).toBe(true)
    expect(confirmTableDeletion('accounts', ' Accounts ')).toBe(false)
    expect(confirmTableDeletion('accounts', 'account')).toBe(false)
  })
})

describe('normalizeImportRows', () => {
  it('normalizes keys into safe lowercase identifiers and preserves values', () => {
    expect(
      normalizeImportRows([
        { 'Customer Name': 'Ada', Revenue$: 25 },
        { 'Customer Name': 'Lin', Revenue$: 30 },
      ]),
    ).toEqual({
      columns: ['customer_name', 'revenue'],
      rows: [
        { customer_name: 'Ada', revenue: 25 },
        { customer_name: 'Lin', revenue: 30 },
      ],
      rowCount: 2,
    })
  })

  it('drops headers that cannot become valid identifiers', () => {
    expect(
      normalizeImportRows([
        { '!!!': 'ignored', 'Account ID': 42, 'Plan.Name': 'pro' },
      ]),
    ).toEqual({
      columns: ['account_id', 'plan_name'],
      rows: [{ account_id: 42, plan_name: 'pro' }],
      rowCount: 1,
    })
  })

  it('returns an empty import model for no rows', () => {
    expect(normalizeImportRows([])).toEqual({
      columns: [],
      rows: [],
      rowCount: 0,
    })
  })
})

describe('canEditTableRows', () => {
  it('allows editing only when a safe id column exists', () => {
    expect(
      canEditTableRows(
        {
          columns: [
            { name: 'id', primary_key: true },
            { name: 'name', primary_key: false },
          ],
        },
        [{ id: 1, name: 'Ada' }],
      ),
    ).toEqual({
      editable: true,
      reason: null,
    })

    expect(
      canEditTableRows(
        {
          columns: [{ name: 'name', primary_key: false }],
        },
        [{ name: 'Ada' }],
      ),
    ).toEqual({
      editable: false,
      reason: "Editing requires an 'id' column in the current table.",
    })
  })

  it('requires every row to carry an id before enabling edits', () => {
    expect(
      canEditTableRows(
        {
          columns: [{ name: 'id', primary_key: true }],
        },
        [{ id: 1 }, { name: 'missing id' }],
      ),
    ).toEqual({
      editable: false,
      reason: "Editing requires an 'id' column in the current table.",
    })
  })
})

describe('inferTableRelationships', () => {
  it('infers singular and plural id-based relationships', () => {
    expect(
      inferTableRelationships([
        { name: 'customers', columns: [{ name: 'id' }] },
        { name: 'orders', columns: [{ name: 'customer_id' }] },
        { name: 'audit', columns: [{ name: 'orders_id' }] },
      ]),
    ).toEqual([
      {
        sourceTable: 'orders',
        sourceColumn: 'customer_id',
        targetTable: 'customers',
      },
      {
        sourceTable: 'audit',
        sourceColumn: 'orders_id',
        targetTable: 'orders',
      },
    ])
  })

  it('ignores id-like columns without a target table', () => {
    expect(
      inferTableRelationships([
        { name: 'events', columns: [{ name: 'workspace_id' }] },
      ]),
    ).toEqual([])
  })
})
