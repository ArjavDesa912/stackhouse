import React, { useRef } from 'react';
import { 
    FileDown, 
    FileUp, 
    File, 
    Save, 
    Settings, 
    ChevronDown,
    PlusCircle,
    Database,
    Share2
} from 'lucide-react';
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Button } from "@/components/ui/button";

export default function FileMenu({ onSave, onLoad, config }) {
    const fileInputRef = useRef(null);

    const handleExport = () => {
        const dataStr = "data:text/json;charset=utf-8," + encodeURIComponent(JSON.stringify(config, null, 2));
        const downloadAnchorNode = document.createElement('a');
        downloadAnchorNode.setAttribute("href", dataStr);
        downloadAnchorNode.setAttribute("download", "workbook_config.json");
        document.body.appendChild(downloadAnchorNode);
        downloadAnchorNode.click();
        downloadAnchorNode.remove();
    };

    const handleFileChange = (e) => {
        const file = e.target.files[0];
        if (!file) return;

        const reader = new FileReader();
        reader.onload = (event) => {
            try {
                const json = JSON.parse(event.target.result);
                onLoad(json);
            } catch (err) {
                alert("Invalid JSON file");
            }
        };
        reader.readAsText(file);
        e.target.value = null; // Reset
    };

    return (
        <div className="relative">
            <DropdownMenu>
                <DropdownMenuTrigger asChild>
                    <Button variant="ghost" size="sm" className="h-6 gap-1 text-xs font-bold text-muted-foreground hover:text-foreground transition-colors group">
                        <File className="w-2.5 h-2.5 opacity-60 group-hover:opacity-100 transition-opacity" />
                        File
                        <ChevronDown className="w-2.5 h-2.5 opacity-40 group-hover:opacity-100 transition-opacity" />
                    </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-40 p-1 rounded-lg border-border bg-card/80 backdrop-blur-3xl shadow-none animate-in zoom-in-95 duration-200">
                    <DropdownMenuLabel className="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase tracking-widest">Workspace Management</DropdownMenuLabel>
                    
                    <DropdownMenuItem 
                        onClick={() => onLoad(null)}
                        className="flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer focus:bg-primary/5 focus:text-primary transition-all group"
                    >
                        <div className="w-6 h-6 rounded-md bg-primary/10 flex items-center justify-center text-primary border border-primary/10 group-hover:scale-110 transition-transform">
                            <PlusCircle className="w-3 h-3" />
                        </div>
                        <div className="flex flex-col">
                            <span className="text-xs font-bold">New Workbook</span>
                            <span className="text-[9px] text-muted-foreground font-medium uppercase tracking-tight">Clear current context</span>
                        </div>
                    </DropdownMenuItem>

                    <DropdownMenuItem 
                        onClick={() => fileInputRef.current.click()}
                        className="flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer focus:bg-primary/5 focus:text-primary transition-all group"
                    >
                        <div className="w-6 h-6 rounded-md bg-primary/10 flex items-center justify-center text-primary border border-primary/10 group-hover:scale-110 transition-transform">
                            <FileUp className="w-3 h-3" />
                        </div>
                        <div className="flex flex-col">
                            <span className="text-xs font-bold">Open JSON...</span>
                            <span className="text-[9px] text-muted-foreground font-medium uppercase tracking-tight">Load from local machine</span>
                        </div>
                    </DropdownMenuItem>

                    <DropdownMenuSeparator className="my-1 bg-border/50" />
                    <DropdownMenuLabel className="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase tracking-widest">Persistence</DropdownMenuLabel>

                    <DropdownMenuItem 
                        onClick={onSave}
                        className="flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer focus:bg-success/5 focus:text-success transition-all group"
                    >
                        <div className="w-6 h-6 rounded-md bg-success/10 flex items-center justify-center text-success border border-success/10 group-hover:scale-110 transition-transform">
                            <Save className="w-3 h-3" />
                        </div>
                        <div className="flex flex-col">
                            <span className="text-xs font-bold">Save to Cloud</span>
                            <span className="text-[9px] text-muted-foreground font-medium uppercase tracking-tight">Sync with Stackhouse cluster</span>
                        </div>
                    </DropdownMenuItem>

                    <DropdownMenuItem 
                        onClick={handleExport}
                        className="flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer focus:bg-primary/15 focus:text-primary transition-all group"
                    >
                        <div className="w-6 h-6 rounded-md bg-primary/15 flex items-center justify-center text-primary border border-primary/10 group-hover:scale-110 transition-transform">
                            <FileDown className="w-3 h-3" />
                        </div>
                        <div className="flex flex-col">
                            <span className="text-xs font-bold">Export Config</span>
                            <span className="text-[9px] text-muted-foreground font-medium uppercase tracking-tight">Download as .json file</span>
                        </div>
                    </DropdownMenuItem>

                    <DropdownMenuSeparator className="my-1 bg-border/50" />
                    
                    <DropdownMenuItem 
                        className="flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer focus:bg-muted font-medium text-muted-foreground/60 focus:text-foreground opacity-50 grayscale transition-all"
                        disabled
                    >
                        <div className="w-6 h-6 rounded-md bg-muted flex items-center justify-center">
                            <Settings className="w-3 h-3" />
                        </div>
                        <span className="text-xs font-bold uppercase tracking-widest">Global Settings</span>
                    </DropdownMenuItem>
                </DropdownMenuContent>
            </DropdownMenu>

            <input
                type="file"
                ref={fileInputRef}
                className="hidden"
                accept=".json"
                onChange={handleFileChange}
            />
        </div>
    );
}
