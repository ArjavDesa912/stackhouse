import { describe, expect, it } from 'vitest'

import {
  CHART_CATALOG,
  CHART_GROUPS,
  CHART_PALETTE,
  getChartSpec,
  pickAutoChart,
} from './chartCatalog'

describe('CHART_CATALOG', () => {
  it('defines unique chart ids with valid groups and render metadata', () => {
    const ids = new Set()

    CHART_CATALOG.forEach((chart) => {
      expect(ids.has(chart.id)).toBe(false)
      ids.add(chart.id)
      expect(CHART_GROUPS).toContain(chart.group)
      expect(chart.label).toBeTruthy()
      expect(chart.description).toBeTruthy()
      expect(chart.icon).toBeTruthy()
      expect(chart.dims).toBeGreaterThanOrEqual(0)
      expect(chart.measures).toBeGreaterThanOrEqual(0)
    })

    expect(ids.size).toBe(CHART_CATALOG.length)
  })

  it('keeps the palette populated with hex colors', () => {
    expect(CHART_PALETTE.length).toBeGreaterThanOrEqual(8)
    CHART_PALETTE.forEach((color) => {
      expect(color).toMatch(/^#[0-9a-f]{6}$/i)
    })
  })
})

describe('getChartSpec', () => {
  it('returns requested chart specs and falls back to auto', () => {
    expect(getChartSpec('bar')).toMatchObject({
      id: 'bar',
      label: 'Bar',
      group: 'Compare',
    })
    expect(getChartSpec('missing')).toMatchObject({
      id: 'auto',
      label: 'Auto',
    })
  })
})

describe('pickAutoChart', () => {
  it.each([
    [{ dims: 0, measures: 1 }, 'kpi'],
    [{ dims: 0, measures: 2 }, 'scatter'],
    [{ dims: 0, measures: 3 }, 'bubble'],
    [{ dims: 1, measures: 1 }, 'bar'],
    [{ dims: 1, measures: 2 }, 'barGrouped'],
    [{ dims: 2, measures: 1 }, 'heatmap'],
    [{ dims: 0, measures: 0 }, 'bar'],
  ])('chooses %s for available fields', (input, expected) => {
    expect(pickAutoChart(input)).toBe(expected)
  })
})
