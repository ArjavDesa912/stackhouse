function sanitizeIdentifier(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
}

export function buildCreateTableSql(tableName, columns) {
  const columnDefs = columns.map((column) => {
    const constraints = [
      `${column.name} ${column.type}`,
      column.pk ? 'PRIMARY KEY' : '',
      !column.pk && column.notNull ? 'NOT NULL' : '',
    ].filter(Boolean)

    return constraints.join(' ')
  })

  return `CREATE TABLE ${tableName} (${columnDefs.join(', ')})`
}

export function buildAddColumnSql(tableName, column) {
  const constraints = [
    `${column.name} ${column.type}`,
    column.notNull ? 'NOT NULL' : '',
  ].filter(Boolean)

  return `ALTER TABLE ${tableName} ADD COLUMN ${constraints.join(' ')}`
}

export function confirmTableDeletion(tableName, typedValue) {
  return tableName === typedValue
}

export function normalizeImportRows(rows) {
  const normalizedRows = rows.map((row) => {
    const normalizedRow = {}

    Object.entries(row || {}).forEach(([key, value]) => {
      const normalizedKey = sanitizeIdentifier(key)
      if (normalizedKey) {
        normalizedRow[normalizedKey] = value
      }
    })

    return normalizedRow
  })

  const columns = normalizedRows.length > 0 ? Object.keys(normalizedRows[0]) : []

  return {
    columns,
    rows: normalizedRows,
    rowCount: normalizedRows.length,
  }
}

export function canEditTableRows(table, rows) {
  const hasIdColumn = (table?.columns || []).some((column) => column.name === 'id')
  const hasRowIds = rows.every((row) => row && Object.prototype.hasOwnProperty.call(row, 'id'))

  if (!hasIdColumn || !hasRowIds) {
    return {
      editable: false,
      reason: "Editing requires an 'id' column in the current table.",
    }
  }

  return {
    editable: true,
    reason: null,
  }
}

export function inferTableRelationships(tables) {
  const tableNames = new Set((tables || []).map((table) => table.name))

  return (tables || []).flatMap((table) =>
    (table.columns || [])
      .filter((column) => column.name.endsWith('_id'))
      .map((column) => {
        const singular = column.name.replace(/_id$/, '')
        const plural = `${singular}s`
        const target = tableNames.has(plural) ? plural : tableNames.has(singular) ? singular : null

        if (!target) {
          return null
        }

        return {
          sourceTable: table.name,
          sourceColumn: column.name,
          targetTable: target,
        }
      })
      .filter(Boolean),
  )
}
