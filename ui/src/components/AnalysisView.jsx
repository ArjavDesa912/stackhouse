import { ExplainData } from './ExplainData';
import React, { useState, useEffect } from 'react';
import { BarChart, Bar, CartesianGrid, XAxis, YAxis, Tooltip, ResponsiveContainer, PieChart, Pie, Cell } from 'recharts';
import { Activity, Database, Table, AlertTriangle, Sparkles, LayoutDashboard, DatabaseZap } from 'lucide-react';
import { Button } from "@/components/ui/button";
import { apiGet } from '@/lib/apiClient';

const COLORS = [
    'var(--chart-1)',
    'var(--chart-2)',
    'var(--chart-3)',
    'var(--chart-4)',
    'var(--chart-5)',
];

export default function AnalysisView({ tables }) {
    const [selectedTable, setSelectedTable] = useState(tables[0]?.name || '');
    const [stats, setStats] = useState(null);
    const [loading, setLoading] = useState(false);
    const [showExplain, setShowExplain] = useState(false);

    useEffect(() => {
        if (!selectedTable) return;

        async function fetchStats() {
            setLoading(true);
            try {
                const res = await apiGet(`/v1/tables/${selectedTable}`);
                if (res.ok && res.data.success) setStats(res.data.data);
            } catch (e) {
                console.error(e);
            } finally {
                setLoading(false);
            }
        }
        fetchStats();
    }, [selectedTable]);

    if (!tables.length) return (
        <div className="flex flex-col items-center justify-center h-full p-5 text-center">
            <div className="w-12 h-12 rounded-full bg-muted flex items-center justify-center mb-3">
                <DatabaseZap className="w-6 h-6 text-muted-foreground/40" />
            </div>
            <h3 className="text-base font-bold mb-1">No Data for Analysis</h3>
            <p className="text-sm text-muted-foreground max-w-xs">Connect a data source or import a file to begin advanced schema profiling.</p>
        </div>
    );

    const typeDistribution = stats ? Object.entries(
        stats.columns.reduce((acc, col) => {
            acc[col.col_type] = (acc[col.col_type] || 0) + 1;
            return acc;
        }, {})
    ).map(([name, value]) => ({ name, value })) : [];

    return (
        <div className="flex w-full h-full bg-background overflow-hidden">
            {/* Sidebar List */}
            <div className="w-64 border-r border-border bg-card/30 backdrop-blur-xl flex flex-col shadow-none">
                <div className="p-3 border-b border-border flex items-center justify-between">
                    <div className="flex items-center gap-2">
                        <div className="w-6 h-6 rounded-md bg-primary/10 flex items-center justify-center border border-primary/20">
                            <LayoutDashboard className="w-3 h-3 text-primary" />
                        </div>
                        <h2 className="font-bold tracking-tight">Analytics</h2>
                    </div>
                </div>
                <div className="flex-1 overflow-y-auto p-2 space-y-0.5 custom-scrollbar">
                    {tables.map(t => (
                        <button
                            key={t.name}
                            onClick={() => setSelectedTable(t.name)}
                            className={`w-full text-left px-3 py-2 rounded-md text-sm font-medium flex items-center gap-2 transition-all duration-200 group ${selectedTable === t.name ? 'bg-primary text-primary-foreground shadow-none shadow-none scale-[1.02]' : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'}`}
                        >
                            <Table className={`w-3 h-3 transition-transform group-hover:scale-110 ${selectedTable === t.name ? 'opacity-100' : 'opacity-40'}`} />
                            <span className="truncate">{t.name}</span>
                        </button>
                    ))}
                </div>
            </div>

            {/* Main Content */}
            <div className="flex-1 overflow-y-auto bg-muted/5 custom-scrollbar">
                <div className="max-w-5xl mx-auto p-5 space-y-5">
                    {loading ? (
                        <div className="flex flex-col items-center justify-center h-40 gap-3">
                            <Activity className="w-8 h-8 text-primary animate-pulse" />
                            <span className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">Synthesizing Profiler Data...</span>
                        </div>
                    ) : stats ? (
                        <div className="animate-in fade-in slide-in-from-bottom-4 duration-700">
                            <div className="flex items-end justify-between mb-4">
                                <div>
                                    <div className="flex items-center gap-1 mb-1">
                                        <Badge variant="outline" className="text-[9px] font-bold uppercase tracking-widest bg-primary/5 text-primary border-primary/20 rounded-full py-0">Table Analysis</Badge>
                                    </div>
                                    <h1 className="text-2xl font-serif font-medium tracking-tight flex items-center gap-1">
                                        {selectedTable}
                                    </h1>
                                </div>
                                <Button
                                    onClick={() => setShowExplain(!showExplain)}
                                    className={`rounded-lg h-10 px-3 font-bold shadow-none transition-all ${showExplain ? 'bg-primary text-primary-foreground shadow-none-primary/30' : 'bg-background hover:bg-muted border-border border-2 text-foreground shadow-none'}`}
                                >
                                    <Sparkles className="w-3 h-3 mr-1" /> {showExplain ? 'Hide Analysis' : 'Explain with AI'}
                                </Button>
                            </div>

                            {showExplain && (
                                <div className="mb-5 animate-in zoom-in-95 duration-500">
                                    <div className="bg-primary/5 border-2 border-primary/10 rounded-2xl overflow-hidden shadow-none">
                                        <div className="px-3 py-3 bg-primary/10 border-b border-primary/10 flex items-center gap-1">
                                            <Sparkles className="w-3 h-3 text-primary" />
                                            <span className="text-xs font-bold text-primary uppercase tracking-widest">Deep-Dive Analysis</span>
                                        </div>
                                        <div className="p-1">
                                            <ExplainData contextData={stats} />
                                        </div>
                                    </div>
                                </div>
                            )}

                            {/* Header Stats */}
                            <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-5">
                                <div className="bg-card border border-border p-3 rounded-[2rem] shadow-none hover:shadow-none transition-all group">
                                    <div className="flex items-center gap-2 mb-3">
                                        <div className="w-6 h-6 rounded-md bg-primary/10 flex items-center justify-center">
                                            <Activity className="w-3 h-3 text-primary" />
                                        </div>
                                        <div className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">Total Rows</div>
                                    </div>
                                    <div className="text-2xl font-serif font-medium text-foreground tracking-tight group-hover:scale-[1.02] transition-transform">{stats.row_count.toLocaleString()}</div>
                                    <div className="h-0.5 w-full bg-muted rounded-full mt-3 overflow-hidden">
                                        <div className="h-full bg-primary w-2/4 rounded-full"></div>
                                    </div>
                                </div>
                                <div className="bg-card border border-border p-3 rounded-[2rem] shadow-none hover:shadow-none transition-all group">
                                    <div className="flex items-center gap-2 mb-3">
                                        <div className="w-6 h-6 rounded-md bg-primary/10 flex items-center justify-center">
                                            <Table className="w-3 h-3 text-primary" />
                                        </div>
                                        <div className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">Parameters</div>
                                    </div>
                                    <div className="text-2xl font-serif font-medium text-foreground tracking-tight group-hover:scale-[1.02] transition-transform">{stats.column_count}</div>
                                    <div className="h-0.5 w-full bg-muted rounded-full mt-3 overflow-hidden">
                                        <div className="h-full bg-primary w-0.5/2 rounded-full"></div>
                                    </div>
                                </div>
                                <div className="bg-card border border-border p-3 rounded-[2rem] shadow-none hover:shadow-none transition-all group border-success/20 bg-success/5">
                                    <div className="flex items-center gap-2 mb-3">
                                        <div className="w-6 h-6 rounded-md bg-success/10 flex items-center justify-center">
                                            <Activity className="w-3 h-3 text-success" />
                                        </div>
                                        <div className="text-[10px] font-bold text-success dark:text-success uppercase tracking-widest">Health Score</div>
                                    </div>
                                    <div className="text-2xl font-serif font-medium text-success dark:text-success tracking-tight group-hover:scale-[1.02] transition-transform">98%</div>
                                    <div className="h-0.5 w-full bg-muted rounded-full mt-3 overflow-hidden">
                                        <div className="h-full bg-success w-[98%] rounded-full"></div>
                                    </div>
                                </div>
                            </div>

                            {/* Column Types Chart */}
                            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                                <div className="bg-card border border-border p-4 rounded-[2.5rem] shadow-none flex flex-col hover:shadow-none transition-all duration-500">
                                    <div className="flex items-center justify-between mb-4">
                                        <h3 className="text-sm font-bold uppercase tracking-widest text-foreground/70">Type Matrix</h3>
                                        <div className="flex gap-0.5">
                                            <div className="w-0.5.5 h-0.5.5 rounded-full bg-primary animate-pulse"></div>
                                            <div className="w-0.5.5 h-0.5.5 rounded-full bg-primary/40"></div>
                                        </div>
                                    </div>
                                    <div className="flex-1 min-h-[250px] relative">
                                        <ResponsiveContainer width="100%" height="100%">
                                            <PieChart>
                                                <Pie
                                                    data={typeDistribution}
                                                    innerRadius={80}
                                                    outerRadius={100}
                                                    paddingAngle={8}
                                                    dataKey="value"
                                                    stroke="none"
                                                >
                                                    {typeDistribution.map((entry, index) => (
                                                        <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
                                                    ))}
                                                </Pie>
                                                <Tooltip 
                                                    contentStyle={{ 
                                                        backgroundColor: 'hsl(var(--card))', 
                                                        borderColor: 'hsl(var(--border))',
                                                        borderRadius: '16px',
                                                        boxShadow: '0 20px 25px -5px rgb(0 0 0 / 0.1)',
                                                        border: '1px solid hsl(var(--border))'
                                                    }} 
                                                />
                                            </PieChart>
                                        </ResponsiveContainer>
                                        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
                                            <span className="text-xl font-serif font-medium tracking-tight">{stats.column_count}</span>
                                            <span className="text-[8px] font-bold text-muted-foreground uppercase tracking-widest">Attributes</span>
                                        </div>
                                    </div>
                                    <div className="flex flex-wrap gap-3 justify-center mt-4">
                                        {typeDistribution.map((entry, index) => (
                                            <div key={entry.name} className="flex items-center gap-1 text-[10px] font-bold text-muted-foreground uppercase tracking-tight group cursor-default">
                                                <div className="w-1.5 h-1.5 rounded-full ring-2 ring-offset-2 ring-offset-background group-hover:scale-125 transition-transform" style={{ backgroundColor: COLORS[index % COLORS.length], ringColor: COLORS[index % COLORS.length] }} />
                                                {entry.name}
                                            </div>
                                        ))}
                                    </div>
                                </div>

                                <div className="bg-card border border-border p-4 rounded-[2.5rem] shadow-none flex flex-col hover:shadow-none transition-all duration-500">
                                    <div className="flex items-center justify-between mb-4">
                                        <h3 className="text-sm font-bold uppercase tracking-widest text-foreground/70">Data Dictionary</h3>
                                        <div className="px-2 py-0.5 bg-muted rounded-full text-[9px] font-bold text-muted-foreground uppercase tracking-tight">Active Schema</div>
                                    </div>
                                    <div className="flex-1 min-h-0 overflow-y-auto custom-scrollbar">
                                        <table className="w-full text-left">
                                            <thead>
                                                <tr className="text-[9px] font-bold text-muted-foreground uppercase tracking-[0.2em] border-b border-border">
                                                    <th className="pb-3">Column</th>
                                                    <th className="pb-3">Format</th>
                                                    <th className="pb-3">Rules</th>
                                                </tr>
                                            </thead>
                                            <tbody className="divide-y divide-border">
                                                {stats.columns.map(col => (
                                                    <tr key={col.name} className="group hover:bg-muted/30 transition-colors">
                                                        <td className="py-3 text-xs font-bold text-primary tracking-tight font-mono group-hover:translate-x-1 transition-transform">{col.name}</td>
                                                        <td className="py-3 text-[10px] font-bold text-muted-foreground uppercase tracking-tight">{col.col_type}</td>
                                                        <td className="py-3">
                                                            <div className="flex gap-0.5.5 flex-wrap">
                                                                {col.primary_key && <Badge variant="secondary" className="bg-success/10 text-success dark:text-success border-none text-[8px] font-semibold px-0.5.5 py-0 leading-none h-3">PK</Badge>}
                                                                {!col.nullable && <Badge variant="secondary" className="bg-primary/15 text-primary dark:text-primary border-none text-[8px] font-semibold px-0.5.5 py-0 leading-none h-3">NULL</Badge>}
                                                                {!col.primary_key && col.nullable && <span className="text-muted-foreground/20 text-[10px]">--</span>}
                                                            </div>
                                                        </td>
                                                    </tr>
                                                ))}
                                            </tbody>
                                        </table>
                                    </div>
                                </div>
                            </div>
                        </div>
                    ) : null}
                </div>
            </div>
        </div>
    );
}

function Badge({ children, variant = "default", className = "" }) {
  const variants = {
    default: "bg-primary text-primary-foreground",
    secondary: "bg-secondary text-secondary-foreground",
    outline: "text-foreground border border-input bg-background"
  };
  return (
    <div className={`inline-flex items-center px-1.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 \${variants[variant]} \${className}`}>
      {children}
    </div>
  );
}
