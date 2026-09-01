import {
  Sparkles,
  BarChart3,
  BarChartHorizontal,
  BarChartBig,
  Layers,
  Percent,
  LineChart as LineIcon,
  Spline,
  TrendingUp,
  AreaChart as AreaIcon,
  Mountain,
  PieChart as PieIcon,
  CircleDot,
  Donut,
  Grid3x3,
  FolderTree,
  Filter,
  ScatterChart,
  Circle,
  Dices,
  Radar as RadarIcon,
  Activity,
  GitCompare,
  Waves,
  TrendingDown,
  LayoutGrid,
  Gauge as GaugeIcon,
  Hash,
  TableProperties,
  Globe,
  MapPin,
  Map as MapIcon,
} from 'lucide-react'

/**
 * Central catalog of visualization types supported by the Visual Worksheet.
 * `dims` and `measures` express the minimum field count required to render.
 * `maxDims` / `maxMeasures` (when set) soft-cap how many are used.
 */
export const CHART_CATALOG = [
  // --- Auto / fallback
  { id: 'auto', label: 'Auto', group: 'Smart', icon: Sparkles, description: 'Pick a sensible chart from the fields you assigned.', dims: 0, measures: 0 },

  // --- Compare categories
  { id: 'bar',           label: 'Bar',                group: 'Compare', icon: BarChart3,            description: 'Compare a measure across categories.',                 dims: 1, measures: 1 },
  { id: 'barHorizontal', label: 'Horizontal Bar',     group: 'Compare', icon: BarChartHorizontal,   description: 'Same as Bar, rotated — good for long labels.',        dims: 1, measures: 1 },
  { id: 'barGrouped',    label: 'Grouped Bar',        group: 'Compare', icon: BarChartBig,             description: 'Compare two or more measures side-by-side per group.', dims: 1, measures: 2 },
  { id: 'barStacked',    label: 'Stacked Bar',        group: 'Compare', icon: Layers,               description: 'Stack multiple measures to show composition totals.', dims: 1, measures: 2 },
  { id: 'bar100',        label: '100% Stacked Bar',   group: 'Compare', icon: Percent,              description: 'Compare part-to-whole composition as percentages.',   dims: 1, measures: 2 },

  // --- Trends
  { id: 'line',         label: 'Line',              group: 'Trend', icon: LineIcon,    description: 'Show a measure evolving across an ordered dimension.',  dims: 1, measures: 1 },
  { id: 'lineMulti',    label: 'Multi-Line',        group: 'Trend', icon: TrendingUp,  description: 'Compare trends across multiple measures.',              dims: 1, measures: 2 },
  { id: 'step',         label: 'Step Line',         group: 'Trend', icon: Spline,      description: 'Good for state changes / stepwise values.',             dims: 1, measures: 1 },
  { id: 'area',         label: 'Area',              group: 'Trend', icon: AreaIcon,    description: 'A line chart with a filled area beneath.',              dims: 1, measures: 1 },
  { id: 'areaStacked',  label: 'Stacked Area',      group: 'Trend', icon: Mountain,    description: 'Stack multiple measures to show evolution of totals.',  dims: 1, measures: 2 },
  { id: 'area100',      label: '100% Stacked Area', group: 'Trend', icon: Waves,       description: 'Composition over time as percentages.',                 dims: 1, measures: 2 },

  // --- Part of whole
  { id: 'pie',       label: 'Pie',         group: 'Part-of-Whole', icon: PieIcon,    description: 'Share of total for a single measure across categories.', dims: 1, measures: 1 },
  { id: 'donut',     label: 'Donut',       group: 'Part-of-Whole', icon: Donut,      description: 'Like a pie with a hollow core — room for a KPI.',        dims: 1, measures: 1 },
  { id: 'radialBar', label: 'Radial Bar',  group: 'Part-of-Whole', icon: CircleDot,  description: 'Bars drawn along a circular axis.',                      dims: 1, measures: 1 },
  { id: 'treemap',   label: 'Treemap',     group: 'Part-of-Whole', icon: FolderTree, description: 'Nested rectangles sized by a measure.',                  dims: 1, measures: 1 },
  { id: 'funnel',    label: 'Funnel',      group: 'Part-of-Whole', icon: Filter,     description: 'Show stage-to-stage drop-off in a pipeline.',            dims: 1, measures: 1 },

  // --- Distribution
  { id: 'histogram', label: 'Histogram', group: 'Distribution', icon: Hash,           description: 'Bucket a numeric measure into bins.',                       dims: 0, measures: 1 },
  { id: 'scatter',   label: 'Scatter',   group: 'Distribution', icon: ScatterChart,   description: 'Plot two measures against each other to see correlation.',  dims: 0, measures: 2 },
  { id: 'bubble',    label: 'Bubble',    group: 'Distribution', icon: Circle,         description: 'Scatter with a third measure controlling marker size.',     dims: 0, measures: 3 },
  { id: 'radar',     label: 'Radar',     group: 'Distribution', icon: RadarIcon,      description: 'Profile categories across several axes.',                   dims: 1, measures: 1 },

  // --- Hybrid / advanced
  { id: 'composed',  label: 'Bar + Line',    group: 'Hybrid', icon: GitCompare, description: 'Mix a bar and a line for dual-axis comparisons.',      dims: 1, measures: 2 },
  { id: 'waterfall', label: 'Waterfall',     group: 'Hybrid', icon: TrendingDown,     description: 'Running total of sequential positive / negative steps.', dims: 1, measures: 1 },
  { id: 'pareto',    label: 'Pareto',        group: 'Hybrid', icon: Activity,         description: 'Sorted bars with cumulative % — 80/20 analysis.',       dims: 1, measures: 1 },

  // --- Matrix
  { id: 'heatmap',   label: 'Heatmap',       group: 'Matrix', icon: Grid3x3,     description: 'Intensity grid for two dimensions × one measure.',            dims: 2, measures: 1 },

  // --- Geospatial
  { id: 'mapChoropleth', label: 'World Choropleth', group: 'Geo', icon: Globe,    description: 'Color countries on a world map by a measure. Dimension should be ISO-3 country code or name.', dims: 1, measures: 1 },
  { id: 'mapBubble',     label: 'Bubble Map',       group: 'Geo', icon: MapIcon,  description: 'Plot bubbles on a world map. Expects latitude, longitude, and a size measure.',                 dims: 0, measures: 3 },
  { id: 'mapPoints',     label: 'Point Map',        group: 'Geo', icon: MapPin,   description: 'Plot simple points on a world map. Expects latitude + longitude measures.',                     dims: 0, measures: 2 },

  // --- Summary
  { id: 'kpi',       label: 'KPI Card',      group: 'Summary', icon: Dices,            description: 'Single big number for the total of a measure.',             dims: 0, measures: 1 },
  { id: 'gauge',     label: 'Gauge',         group: 'Summary', icon: GaugeIcon,        description: 'Semicircle showing a measure as a % of a target.',          dims: 0, measures: 1 },
  { id: 'sparkGrid', label: 'Sparkline Grid',group: 'Summary', icon: LayoutGrid,       description: 'Small line cards for each category plus a headline total.', dims: 1, measures: 1 },
  { id: 'table',     label: 'Data Table',    group: 'Summary', icon: TableProperties,  description: 'Raw tabular preview of the underlying rows.',                dims: 0, measures: 0 },
]

export const CHART_GROUPS = [
  'Smart',
  'Compare',
  'Trend',
  'Part-of-Whole',
  'Distribution',
  'Hybrid',
  'Matrix',
  'Geo',
  'Summary',
]

export function getChartSpec(id) {
  return CHART_CATALOG.find((c) => c.id === id) || CHART_CATALOG[0]
}

export function pickAutoChart({ dims, measures }) {
  if (dims === 0 && measures === 1) return 'kpi'
  if (dims === 0 && measures === 2) return 'scatter'
  if (dims === 0 && measures >= 3) return 'bubble'
  if (dims === 1 && measures === 1) return 'bar'
  if (dims === 1 && measures >= 2) return 'barGrouped'
  if (dims >= 2 && measures >= 1) return 'heatmap'
  return 'bar'
}

// Literal hex, not CSS vars: consumers append an alpha suffix (`${color}33`)
// for fill opacity, which only works on hex strings. Values are mirrors of
// the real design tokens in styles/tokens.css (--flux, --success, --info,
// --warning, --danger and their -deep variants, plus --ink-2/--ink-3) so the
// categorical chart palette stays the same color language as the rest of
// the UI. Keep in sync if those tokens change.
export const CHART_PALETTE = [
  '#0fb5a6', // --flux
  '#1f9d6e', // --success
  '#2e6bd6', // --info
  '#d8881a', // --warning
  '#d2362f', // --danger
  '#0a8a7e', // --flux-deep
  '#4b4f58', // --ink-2
  '#8a8e97', // --ink-3
  '#166f4f', // --success-deep
  '#1f4b9c', // --info-deep
]
