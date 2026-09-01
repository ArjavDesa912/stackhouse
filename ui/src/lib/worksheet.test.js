import { describe, expect, it } from 'vitest'

import {
  addFieldToShelf,
  classifyFieldType,
  deriveWorksheetModel,
  normalizeField,
  removeFieldFromShelf,
} from './worksheet'

describe('classifyFieldType', () => {
  it('classifies text and date columns as dimensions', () => {
    expect(classifyFieldType('TEXT')).toBe('dimension')
    expect(classifyFieldType('DATE')).toBe('dimension')
    expect(classifyFieldType('BOOLEAN')).toBe('dimension')
  })

  it('classifies numeric columns as measures', () => {
    expect(classifyFieldType('INTEGER')).toBe('measure')
    expect(classifyFieldType('DECIMAL')).toBe('measure')
    expect(classifyFieldType('DOUBLE PRECISION')).toBe('measure')
  })
})

describe('normalizeField', () => {
  it('normalizes legacy dim fields to the dimension type', () => {
    expect(
      normalizeField({ name: 'region', col_type: 'TEXT', type: 'dim' }),
    ).toMatchObject({
      name: 'region',
      type: 'dimension',
      role: 'dimension',
    })
  })

  it('preserves explicit measure and dimension roles', () => {
    expect(normalizeField({ name: 'revenue', type: 'measure' })).toMatchObject({
      name: 'revenue',
      type: 'measure',
      role: 'measure',
    })
    expect(normalizeField({ name: 'region', type: 'dimension' })).toMatchObject({
      name: 'region',
      type: 'dimension',
      role: 'dimension',
    })
  })
})

describe('shelf helpers', () => {
  it('adds a field to a shelf only once', () => {
    const field = normalizeField({ name: 'revenue', col_type: 'INTEGER' })

    expect(addFieldToShelf([], field)).toEqual([field])
    expect(addFieldToShelf([field], field)).toEqual([field])
  })

  it('removes a field from a shelf by name', () => {
    const shelf = [
      normalizeField({ name: 'region', col_type: 'TEXT' }),
      normalizeField({ name: 'revenue', col_type: 'INTEGER' }),
    ]

    expect(removeFieldFromShelf(shelf, 'region')).toEqual([
      normalizeField({ name: 'revenue', col_type: 'INTEGER' }),
    ])
  })
})

describe('deriveWorksheetModel', () => {
  it('returns an empty state without data or assignments', () => {
    expect(
      deriveWorksheetModel({
        data: [],
        shelves: { columns: [], rows: [] },
      }),
    ).toEqual({
      status: 'empty',
      chartData: [],
      tableColumns: [],
      tableRows: [],
    })
  })

  it('builds an aggregated preview for one dimension and one measure', () => {
    const model = deriveWorksheetModel({
      data: [
        { region: 'North', revenue: 10 },
        { region: 'North', revenue: 5 },
        { region: 'South', revenue: 7 },
      ],
      shelves: {
        columns: [normalizeField({ name: 'region', col_type: 'TEXT' })],
        rows: [normalizeField({ name: 'revenue', col_type: 'INTEGER' })],
      },
    })

    expect(model.status).toBe('ready')
    expect(model.chartData).toEqual([
      { key: 'North', label: 'North', value: 15 },
      { key: 'South', label: 'South', value: 7 },
    ])
    expect(model.tableColumns).toEqual(['region', 'revenue'])
  })

  it('can use dimensions from rows and measures from columns', () => {
    const model = deriveWorksheetModel({
      data: [
        { region: 'North', revenue: 10 },
        { region: 'North', revenue: 'not numeric' },
        { region: 'South', revenue: 7 },
      ],
      shelves: {
        columns: [normalizeField({ name: 'revenue', col_type: 'INTEGER' })],
        rows: [normalizeField({ name: 'region', col_type: 'TEXT' })],
      },
    })

    expect(model.status).toBe('ready')
    expect(model.chartData).toEqual([
      { key: 'North', label: 'North', value: 10 },
      { key: 'South', label: 'South', value: 7 },
    ])
  })

  it('returns an incomplete state when data exists but the shelves are not chartable', () => {
    const model = deriveWorksheetModel({
      data: [{ revenue: 10 }],
      shelves: {
        columns: [],
        rows: [normalizeField({ name: 'revenue', col_type: 'INTEGER' })],
      },
    })

    expect(model.status).toBe('incomplete')
    expect(model.chartData).toEqual([])
    expect(model.tableRows).toEqual([{ revenue: 10 }])
  })
})
