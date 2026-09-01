const DIMENSION_TYPES = ['text', 'varchar', 'char', 'date', 'datetime', 'timestamp', 'time', 'uuid', 'json', 'bool']
const MEASURE_TYPES = ['int', 'real', 'numeric', 'decimal', 'double', 'float']

function normalizeColumnType(columnType) {
  return (columnType ?? '').toLowerCase()
}

export function classifyFieldType(columnType) {
  const normalizedType = normalizeColumnType(columnType)

  if (MEASURE_TYPES.some((token) => normalizedType.includes(token))) {
    return 'measure'
  }

  if (DIMENSION_TYPES.some((token) => normalizedType.includes(token))) {
    return 'dimension'
  }

  return 'dimension'
}

export function normalizeField(field) {
  const inferredType =
    field?.type === 'dimension' ||
    field?.type === 'measure'
      ? field.type
      : field?.type === 'dim'
        ? 'dimension'
        : classifyFieldType(field?.col_type)

  return {
    ...field,
    type: inferredType,
    role: inferredType,
  }
}

export function addFieldToShelf(shelf, field) {
  const normalizedField = normalizeField(field)

  if (shelf.some((item) => item.name === normalizedField.name)) {
    return shelf
  }

  return [...shelf, normalizedField]
}

export function removeFieldFromShelf(shelf, fieldName) {
  return shelf.filter((item) => item.name !== fieldName)
}

function getAssignedField(shelf, type) {
  return (shelf || []).map(normalizeField).find((field) => field.type === type)
}

export function deriveWorksheetModel({ data = [], shelves = {} }) {
  const columns = shelves.columns || []
  const rows = shelves.rows || []
  const dimensionField = getAssignedField(columns, 'dimension') || getAssignedField(rows, 'dimension')
  const measureField = getAssignedField(rows, 'measure') || getAssignedField(columns, 'measure')

  if (data.length === 0 && columns.length === 0 && rows.length === 0) {
    return {
      status: 'empty',
      chartData: [],
      tableColumns: [],
      tableRows: [],
    }
  }

  const tableColumns = data.length > 0 ? Object.keys(data[0]) : []

  if (!dimensionField || !measureField || data.length === 0) {
    return {
      status: 'incomplete',
      chartData: [],
      tableColumns,
      tableRows: data,
    }
  }

  const totals = new Map()

  data.forEach((row) => {
    const key = row[dimensionField.name]
    const value = Number(row[measureField.name]) || 0
    totals.set(key, (totals.get(key) || 0) + value)
  })

  return {
    status: 'ready',
    chartData: Array.from(totals.entries()).map(([key, value]) => ({
      key,
      label: key,
      value,
    })),
    tableColumns,
    tableRows: data,
  }
}
