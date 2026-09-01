import React, { useState, useEffect } from 'react';
import { Layout, Plus, FileText, Trash2, Calendar, Clock, BarChart3, ArrowUpRight, Loader2 } from 'lucide-react';
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { apiGet, apiPost } from '@/lib/apiClient';

export default function DashboardView({ onLoadConfig }) {
    const [dashboards, setDashboards] = useState([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);

    useEffect(() => {
        loadDashboards();
    }, []);

    const loadDashboards = async () => {
        setLoading(true);
        try {
            const metaRes = await apiGet('/v1/tables');
            const metaJson = metaRes.data;

            if (metaJson.success && metaJson.tables.includes('stackhouse_dashboards')) {
                const res = await apiGet('/v1/query/stackhouse_dashboards?order_by=created_at&order_dir=DESC');
                const json = res.data;
                if (json.success) {
                    setDashboards(json.data);
                }
            } else {
                setDashboards([]);
            }
        } catch (e) {
            console.error(e);
            setError("Failed to load dashboards");
        } finally {
            setLoading(false);
        }
    };

    const handleDelete = async (e, id) => {
        e.stopPropagation();
        if (!confirm("Are you sure you want to delete this dashboard?")) return;

        try {
            await apiPost(`/v1/delete/stackhouse_dashboards/${id}`);
            loadDashboards();
        } catch (e) {
            alert("Failed to delete");
        }
    };

    return (
        <div className="flex-1 bg-background p-5 overflow-y-auto custom-scrollbar">
            <div className="max-w-7xl mx-auto space-y-5">
                <div className="flex items-end justify-between">
                    <div className="space-y-1">
                        <div className="flex items-center gap-2">
                            <div className="w-8 h-8 rounded-lg bg-primary/10 flex items-center justify-center border border-primary/20 shadow-none shadow-none-primary/5">
                                <Layout className="w-4 h-4 text-primary" />
                            </div>
                            <h1 className="text-2xl font-serif font-medium tracking-tight text-foreground">
                                Dashboards
                            </h1>
                        </div>
                        <p className="text-sm text-balance text-muted-foreground font-medium max-w-md">
                            Manage your saved visualizations, reports, and data perspectives in one unified interface.
                        </p>
                    </div>
                    <Button
                        onClick={() => onLoadConfig(null)}
                        size="lg"
                        className="rounded-lg px-4 font-semibold text-xs uppercase tracking-widest shadow-none shadow-none hover:scale-105 transition-all"
                    >
                        <Plus className="w-4 h-4 mr-2" /> New Workspace
                    </Button>
                </div>

                {loading ? (
                    <div className="h-80 flex flex-col items-center justify-center gap-3 animate-in fade-in duration-700">
                        <div className="relative">
                            <Loader2 className="w-12 h-12 animate-spin text-primary opacity-20" />
                            <BarChart3 className="w-5 h-5 absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-primary" />
                        </div>
                        <span className="text-[10px] font-semibold uppercase tracking-[0.3em] text-muted-foreground">Hydrating Visual Graph...</span>
                    </div>
                ) : (
                    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                        {/* Create New Card */}
                        <Card 
                            onClick={() => onLoadConfig(null)}
                            className="h-64 rounded-[2.5rem] border-2 border-dashed border-border bg-muted/5 hover:bg-primary/5 hover:border-primary/40 transition-all cursor-pointer group flex flex-col items-center justify-center text-center space-y-3"
                        >
                            <div className="w-12 h-12 rounded-[1.5rem] bg-card border border-border flex items-center justify-center shadow-none group-hover:scale-110 group-hover:rotate-6 transition-all duration-500">
                                <Plus className="w-6 h-6 text-primary" />
                            </div>
                            <div className="space-y-0.5">
                                <h3 className="text-sm font-semibold uppercase tracking-widest">Create New</h3>
                                <p className="text-[10px] text-muted-foreground font-bold uppercase tracking-tight">Start from absolute zero</p>
                            </div>
                        </Card>

                        {/* Dashboard Cards */}
                        {dashboards.map((d, index) => (
                            <Card
                                key={d.id}
                                onClick={() => {
                                    try {
                                        const config = typeof d.config === 'string' ? JSON.parse(d.config) : d.config;
                                        onLoadConfig(config);
                                    } catch (e) {
                                        alert("Error loading dashboard configuration");
                                    }
                                }}
                                className="h-64 rounded-[2.5rem] bg-card border border-border p-0 flex flex-col hover:shadow-none hover:-translate-y-2 transition-all duration-500 cursor-pointer relative group overflow-hidden animate-in zoom-in-95"
                                style={{ animationDelay: `${index * 50}ms` }}
                            >
                                <div className="absolute top-0 left-0 w-full h-0.5.5 bg-primary opacity-0 group-hover:opacity-100 transition-opacity" />
                                
                                <CardHeader className="p-4 pb-3">
                                    <div className="flex items-start justify-between mb-1">
                                        <div className="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center text-primary border border-primary/20 shadow-none group-hover:scale-110 transition-transform duration-500">
                                            <BarChart3 className="w-5 h-5" />
                                        </div>
                                        <button
                                            onClick={(e) => handleDelete(e, d.id)}
                                            className="p-1 text-muted-foreground hover:text-destructive hover:bg-destructive/10 rounded-md transition-all opacity-0 group-hover:opacity-100 shrink-0"
                                            title="Purge"
                                        >
                                            <Trash2 className="w-3 h-3" />
                                        </button>
                                    </div>
                                    <CardTitle className="text-base font-semibold tracking-tight truncate group-hover:text-primary transition-colors">{d.name || 'Untitled Segment'}</CardTitle>
                                    <CardDescription className="text-[10px] font-bold uppercase tracking-widest flex items-center gap-0.5.5">
                                        <Calendar className="w-2 h-2" />
                                        {new Date(d.created_at).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })}
                                    </CardDescription>
                                </CardHeader>

                                <CardContent className="px-4 pb-4 pt-0 flex-1 flex flex-col justify-end">
                                    <div className="flex items-center gap-1 mb-3">
                                        <Badge variant="outline" className="text-[9px] font-semibold uppercase tracking-tight bg-muted/50 border-border text-muted-foreground px-1 h-4">STACKHOUSE_CORE_V4</Badge>
                                        <Badge variant="outline" className="text-[9px] font-semibold uppercase tracking-tight bg-success/5 border-success/10 text-success px-1 h-4">SYNCED</Badge>
                                    </div>
                                    <div className="flex items-center justify-between text-[11px] font-bold text-muted-foreground/60">
                                        <div className="flex items-center gap-0.5">
                                            <Clock className="w-2.5 h-2.5" />
                                            <span>Ready</span>
                                        </div>
                                        <ArrowUpRight className="w-3 h-3 opacity-0 group-hover:opacity-100 group-hover:translate-x-1 group-hover:-translate-y-1 transition-all" />
                                    </div>
                                </CardContent>
                            </Card>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
