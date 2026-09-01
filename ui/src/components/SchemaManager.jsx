import React, { useMemo, useState } from 'react'

import WorkspaceNav from './schema/WorkspaceNav'
import SchemaCatalogView from './schema/SchemaCatalogView'
import TableDesignerView from './schema/TableDesignerView'
import DataExplorerView from './schema/DataExplorerView'
import RelationshipModelView from './schema/RelationshipModelView'
import ImportStudioView from './schema/ImportStudioView'
import OperationsView from './schema/OperationsView'

export default function SchemaManager({ tables = [], onRefresh }) {
  const [activeWorkspace, setActiveWorkspace] = useState('catalog')
  const [selectedTableName, setSelectedTableName] = useState('')

  const selectedTable = useMemo(
    () => tables.find((table) => table.name === selectedTableName) || null,
    [selectedTableName, tables],
  )

  const renderWorkspace = () => {
    switch (activeWorkspace) {
      case 'designer':
        return (
          <TableDesignerView
            tables={tables}
            selectedTable={selectedTable}
            onRefresh={onRefresh}
            onSelectTable={setSelectedTableName}
          />
        )
      case 'explorer':
        return <DataExplorerView selectedTable={selectedTable} />
      case 'model':
        return (
          <RelationshipModelView
            tables={tables}
            selectedTableName={selectedTableName}
            onSelectTable={setSelectedTableName}
          />
        )
      case 'import':
        return <ImportStudioView onRefresh={onRefresh} onSelectTable={setSelectedTableName} />
      case 'operations':
        return <OperationsView tables={tables} />
      case 'catalog':
      default:
        return (
          <SchemaCatalogView
            tables={tables}
            selectedTableName={selectedTableName}
            onSelectTable={setSelectedTableName}
            onOpenWorkspace={setActiveWorkspace}
            onRefresh={onRefresh}
          />
        )
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground">
      <WorkspaceNav
        activeWorkspace={activeWorkspace}
        onChange={setActiveWorkspace}
        tables={tables}
        selectedTableName={selectedTableName}
        onSelectTable={setSelectedTableName}
      />
      <div className="min-h-0 flex-1 overflow-hidden">{renderWorkspace()}</div>
    </div>
  )
}
