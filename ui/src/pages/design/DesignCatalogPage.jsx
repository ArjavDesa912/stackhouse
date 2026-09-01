import { useState } from 'react';
import {
  Database, Sparkles, Plus, ArrowRight, Search,
  CheckCircle2, AlertTriangle, AlertCircle, Info,
} from 'lucide-react';

import { PageHeader } from '@/components/data-display/PageHeader';
import { StatCard, StatGrid } from '@/components/data-display/StatCard';
import { EmptyState } from '@/components/data-display/EmptyState';
import { Badge } from '@/components/data-display/Badge';
import { Kbd, ShortcutGroup } from '@/components/data-display/KbdShortcut';
import { Skeleton, SkeletonText, SkeletonCard, SkeletonStatGrid, SkeletonTable } from '@/components/feedback/Skeleton';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Switch } from '@/components/ui/switch';
import { Checkbox } from '@/components/ui/checkbox';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Progress } from '@/components/ui/progress';
import { Toggle } from '@/components/ui/toggle';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Alert, AlertTitle, AlertDescription } from '@/components/ui/alert';
import { Separator } from '@/components/ui/separator';
import { Label } from '@/components/ui/label';
import {
  Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue, SelectLabel,
} from '@/components/ui/select';
import {
  Tabs, TabsContent, TabsList, TabsTrigger,
} from '@/components/ui/tabs';
import {
  Table, TableHeader, TableBody, TableRow, TableHead, TableCell,
} from '@/components/ui/table';
import {
  Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger,
} from '@/components/ui/dialog';
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger,
} from '@/components/ui/alert-dialog';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { ScrollArea } from '@/components/ui/scroll-area';
import { toast } from '@/components/ui/toast';

function Section({ id, title, description, children }) {
  return (
    <section id={id} className="scroll-mt-6">
      <header className="mb-2">
        <h2 className="text-display-sm text-foreground">{title}</h2>
        {description ? <p className="text-body-sm text-muted-foreground mt-0.5">{description}</p> : null}
      </header>
      <div className="rounded-md border border-border bg-card p-3">{children}</div>
    </section>
  );
}

function Swatch({ name, varName, fg = false }) {
  return (
    <div className="flex flex-col gap-0.5.5">
      <div
        className="h-10 w-full rounded-md border border-border"
        style={{ backgroundColor: `var(${varName})` }}
        aria-hidden
      />
      <div className="flex items-baseline justify-between">
        <span className={`text-caption font-medium ${fg ? 'text-foreground' : 'text-foreground'}`}>{name}</span>
        <span className="text-caption-mono">{varName}</span>
      </div>
    </div>
  );
}

const SURFACE_TOKENS = [
  { name: 'background', v: '--background' },
  { name: 'card', v: '--card' },
  { name: 'muted', v: '--muted' },
  { name: 'secondary', v: '--secondary' },
];

const TEXT_TOKENS = [
  { name: 'foreground', v: '--foreground' },
  { name: 'muted-foreground', v: '--muted-foreground' },
];

const BRAND_TOKENS = [
  { name: 'primary', v: '--primary' },
  { name: 'primary-foreground', v: '--primary-foreground' },
  { name: 'ring', v: '--ring' },
];

const SEMANTIC_TOKENS = [
  { name: 'success', v: '--success' },
  { name: 'warning', v: '--warning' },
  { name: 'destructive',  v: '--destructive' },
];

export default function DesignCatalogPage() {
  const [pct, setPct] = useState(64);
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState(false);

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        eyebrow="Internal · Dev only"
        title="Design Catalog"
        description="Every primitive, token, and composite component in the Stackhouse design system."
        actions={
          <>
            <Button variant="outline" size="sm" onClick={() => toast.info('Hello from sonner.')}>
              <Sparkles className="h-2.5 w-2.5" /> Test toast
            </Button>
            <Button size="sm" onClick={() => setOpen(true)}>
              <Plus className="h-2.5 w-2.5" /> Open dialog
            </Button>
          </>
        }
      />

      <ScrollArea className="flex-1">
        <div className="mx-auto max-w-6xl space-y-5 px-3 py-4 sm:px-4">
          {/* ── Tokens ───────────────────────────────────── */}
          <Section
            id="tokens"
            title="Color tokens"
            description="Surfaces, text, primary brand, and semantic tones."
          >
            <div className="space-y-3">
              <div>
                <p className="text-caption-mono mb-1">Surfaces</p>
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  {SURFACE_TOKENS.map((t) => (
                    <Swatch key={t.v} name={t.name} varName={t.v} />
                  ))}
                </div>
              </div>
              <div>
                <p className="text-caption-mono mb-1">Text</p>
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  {TEXT_TOKENS.map((t) => (
                    <Swatch key={t.v} name={t.name} varName={t.v} />
                  ))}
                </div>
              </div>
              <div>
                <p className="text-caption-mono mb-1">Brand · primary</p>
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  {BRAND_TOKENS.map((t) => (
                    <Swatch key={t.v} name={t.name} varName={t.v} />
                  ))}
                </div>
              </div>
              <div>
                <p className="text-caption-mono mb-1">Semantic</p>
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  {SEMANTIC_TOKENS.map((t) => (
                    <Swatch key={t.v} name={t.name} varName={t.v} />
                  ))}
                </div>
              </div>
            </div>
          </Section>

          {/* ── Typography ───────────────────────────────── */}
          <Section id="typography" title="Typography" description="Geist sans + Geist mono. Aggressive negative tracking on display sizes.">
            <div className="space-y-2">
              <p className="text-display-lg text-foreground">Build with confidence.</p>
              <p className="text-display-md text-foreground">Display Large — section titles.</p>
              <p className="text-display-sm text-foreground">Display Medium — card cluster headings.</p>
              <p className="text-display-sm text-foreground">Display Small — page subsection.</p>
              <Separator />
              <p className="text-body-md text-foreground">Body Large — lead paragraph.</p>
              <p className="text-body-sm text-foreground">Body Medium — default paragraph.</p>
              <p className="text-body-sm text-muted-foreground">Body Small — secondary, captions.</p>
              <p className="text-caption text-muted-foreground/80">Caption — fine print.</p>
              <p className="text-caption-mono">CAPTION-MONO — eyebrow labels</p>
              <pre className="text-code rounded-md bg-muted px-2 py-1 inline-block">{`SELECT * FROM users WHERE id = 1;`}</pre>
            </div>
          </Section>

          {/* ── Buttons ──────────────────────────────────── */}
          <Section id="buttons" title="Buttons" description="shadcn Button — variants × sizes.">
            <div className="flex flex-wrap items-center gap-2">
              <Button>Default</Button>
              <Button variant="outline">Outline</Button>
              <Button variant="secondary">Secondary</Button>
              <Button variant="ghost">Ghost</Button>
              <Button variant="destructive">Destructive</Button>
              <Button variant="link">Link</Button>
            </div>
            <Separator className="my-3" />
            <div className="flex flex-wrap items-center gap-2">
              <Button size="xs">XS</Button>
              <Button size="sm">SM</Button>
              <Button size="default">Default</Button>
              <Button size="lg">LG</Button>
              <Button size="icon" aria-label="add"><Plus /></Button>
              <Button size="default" disabled>Disabled</Button>
              <Button>
                <Sparkles /> With icon
              </Button>
              <Button variant="outline">
                Continue <ArrowRight />
              </Button>
            </div>
          </Section>

          {/* ── Form controls ────────────────────────────── */}
          <Section id="forms" title="Form controls" description="Input, Textarea, Switch, Checkbox, Radio, Select, Toggle.">
            <div className="grid gap-3 md:grid-cols-2">
              <div className="space-y-1">
                <Label>Email</Label>
                <Input placeholder="you@stackhouse.dev" />
              </div>
              <div className="space-y-1">
                <Label>Search</Label>
                <div className="relative">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-2.5 w-2.5 -translate-y-1/2 text-muted-foreground/80" />
                  <Input className="pl-4" placeholder="Find a table…" />
                </div>
              </div>
              <div className="space-y-1 md:col-span-2">
                <Label>Description</Label>
                <Textarea placeholder="Tell us about this dataset…" />
              </div>
              <div className="space-y-1">
                <Label>Region</Label>
                <Select>
                  <SelectTrigger>
                    <SelectValue placeholder="Pick a region" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectLabel>Americas</SelectLabel>
                      <SelectItem value="us-east-1">us-east-1</SelectItem>
                      <SelectItem value="us-west-2">us-west-2</SelectItem>
                    </SelectGroup>
                    <SelectGroup>
                      <SelectLabel>Europe</SelectLabel>
                      <SelectItem value="eu-west-1">eu-west-1</SelectItem>
                      <SelectItem value="eu-central-1">eu-central-1</SelectItem>
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
              <div className="flex items-center gap-2">
                <Switch id="airplane" />
                <Label htmlFor="airplane">Realtime sync</Label>
              </div>
              <div className="flex items-center gap-2">
                <Checkbox id="terms" />
                <Label htmlFor="terms">I agree to the terms</Label>
              </div>
              <div className="space-y-1 md:col-span-2">
                <Label>Storage tier</Label>
                <RadioGroup defaultValue="hot" className="flex flex-wrap gap-3">
                  {['hot', 'warm', 'cold'].map((tier) => (
                    <div key={tier} className="flex items-center gap-1">
                      <RadioGroupItem id={`tier-${tier}`} value={tier} />
                      <Label htmlFor={`tier-${tier}`} className="capitalize">{tier}</Label>
                    </div>
                  ))}
                </RadioGroup>
              </div>
              <div className="space-y-1">
                <Label>Toggle group</Label>
                <div className="inline-flex gap-0.5 rounded-md border border-border bg-background p-0.5">
                  <Toggle defaultPressed>Day</Toggle>
                  <Toggle>Week</Toggle>
                  <Toggle>Month</Toggle>
                </div>
              </div>
              <div className="space-y-1">
                <Label>Sync progress</Label>
                <Progress value={pct} />
                <div className="flex items-center gap-1">
                  <Button variant="outline" size="xs" onClick={() => setPct((p) => Math.max(0, p - 10))}>−10</Button>
                  <Button variant="outline" size="xs" onClick={() => setPct((p) => Math.min(100, p + 10))}>+10</Button>
                  <span className="text-caption tabular text-muted-foreground/80">{pct}%</span>
                </div>
              </div>
            </div>
          </Section>

          {/* ── Feedback ─────────────────────────────────── */}
          <Section id="feedback" title="Feedback" description="Alerts, toasts, dialogs, popovers.">
            <div className="space-y-2">
              <Alert variant="success" icon={CheckCircle2}>
                <AlertTitle>Backup complete</AlertTitle>
                <AlertDescription>Postgres replication caught up at 12:34 UTC.</AlertDescription>
              </Alert>
              <Alert variant="warning" icon={AlertTriangle}>
                <AlertTitle>Slow query detected</AlertTitle>
                <AlertDescription>One query exceeded 4s on the orders table.</AlertDescription>
              </Alert>
              <Alert variant="destructive" icon={AlertCircle}>
                <AlertTitle>Session expired</AlertTitle>
                <AlertDescription>Auth token expired — sign in again to continue.</AlertDescription>
              </Alert>
            </div>
            <Separator className="my-3" />
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="outline" onClick={() => toast.success('Toast — success!')}>Success toast</Button>
              <Button variant="outline" onClick={() => toast.error('Toast — error!')}>Error toast</Button>
              <Popover>
                <PopoverTrigger asChild>
                  <Button variant="outline">Open popover</Button>
                </PopoverTrigger>
                <PopoverContent className="w-40">
                  <p className="text-body-sm font-medium text-foreground">Stackhouse tip</p>
                  <p className="text-caption text-muted-foreground mt-0.5">Press <Kbd>⌘K</Kbd> to summon the command palette anywhere.</p>
                </PopoverContent>
              </Popover>
              <AlertDialog open={confirm} onOpenChange={setConfirm}>
                <AlertDialogTrigger asChild>
                  <Button variant="destructive">Destructive…</Button>
                </AlertDialogTrigger>
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>Delete this table?</AlertDialogTitle>
                    <AlertDialogDescription>This will drop the table and all rows. This action cannot be undone.</AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel>Cancel</AlertDialogCancel>
                    <AlertDialogAction variant="destructive" onClick={() => { setConfirm(false); toast.success('Deleted (demo).'); }}>
                      Delete table
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          </Section>

          {/* ── Tabs ─────────────────────────────────────── */}
          <Section id="tabs" title="Tabs">
            <Tabs defaultValue="overview">
              <TabsList>
                <TabsTrigger value="overview">Overview</TabsTrigger>
                <TabsTrigger value="schema">Schema</TabsTrigger>
                <TabsTrigger value="queries">Queries</TabsTrigger>
              </TabsList>
              <TabsContent value="overview" className="pt-2 text-body-sm text-muted-foreground">
                Top-level metrics and the latest activity stream.
              </TabsContent>
              <TabsContent value="schema" className="pt-2 text-body-sm text-muted-foreground">
                Browse tables, columns, and relationships.
              </TabsContent>
              <TabsContent value="queries" className="pt-2 text-body-sm text-muted-foreground">
                Saved queries and execution history.
              </TabsContent>
            </Tabs>
          </Section>

          {/* ── Cards & Stats ────────────────────────────── */}
          <Section id="cards" title="Cards & Stats">
            <div className="space-y-3">
              <StatGrid>
                <StatCard label="Tables" value="42" icon={Database} hint="Across all schemas" />
                <StatCard label="Rows" value="1.2M" icon={Database} hint="Across all tables" />
                <StatCard label="Queries today" value="318" icon={Sparkles} hint="last 24 hours" />
                <StatCard label="Storage" value="2.4 GB" icon={Database} hint="of 10 GB plan" />
              </StatGrid>
              <Card>
                <CardHeader>
                  <CardTitle>Card primitive</CardTitle>
                  <CardDescription>Composed via shadcn Card / Header / Content.</CardDescription>
                </CardHeader>
                <CardContent>
                  <p className="text-body-sm text-muted-foreground">
                    Cards inherit the card surface, border, and subtle shadow tokens.
                  </p>
                </CardContent>
              </Card>
            </div>
          </Section>

          {/* ── Empty states ─────────────────────────────── */}
          <Section id="empty" title="Empty states" description="Muted and plain surface variants for zero-states and onboarding.">
            <div className="grid gap-3 md:grid-cols-2">
              <EmptyState
                variant="default"
                eyebrow="What's new"
                title="Explore your warehouse."
                description="Run SQL, build schemas, and create live visualizations."
                action={<Button size="sm" className="bg-card text-primary hover:bg-card/90"><Sparkles className="h-2.5 w-2.5" /> Open Worksheet</Button>}
              />
              <EmptyState
                variant="default"
                eyebrow="Production ready"
                title="Schema is healthy."
                description="No drift detected since the last sync."
              />
              <EmptyState
                variant="plain"
                eyebrow="Empty"
                title="No worksheets yet."
                description="Save your first chart in the Worksheet builder."
              />
              <EmptyState
                variant="plain"
                eyebrow="Onboarding"
                title="Connect your first source."
                description="Stackhouse ingests from Postgres, MySQL, S3, and 30+ SaaS APIs."
              />
            </div>
          </Section>

          {/* ── Badges & Kbd ─────────────────────────────── */}
          <Section id="badges" title="Badges & Shortcuts">
            <div className="flex flex-wrap items-center gap-1">
              <Badge>neutral</Badge>
              <Badge tone="brand" dot>brand</Badge>
              <Badge tone="success" dot>synced</Badge>
              <Badge tone="warning" dot>pending</Badge>
              <Badge tone="destructive" dot>failed</Badge>
            </div>
            <Separator className="my-3" />
            <div className="flex flex-wrap items-center gap-2 text-body-sm text-muted-foreground">
              <span>Open palette</span>
              <ShortcutGroup keys={['⌘', 'K']} />
              <span>Send</span>
              <ShortcutGroup keys={['⌘', 'Enter']} />
              <span>Submit</span>
              <Kbd>Enter</Kbd>
            </div>
          </Section>

          {/* ── Skeletons ────────────────────────────────── */}
          <Section id="skeleton" title="Skeletons">
            <div className="space-y-3">
              <SkeletonStatGrid />
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <SkeletonCard />
                <div className="rounded-md border border-border bg-card p-3">
                  <SkeletonText lines={4} />
                </div>
              </div>
              <SkeletonTable rows={4} cols={5} />
            </div>
          </Section>

          {/* ── Table ────────────────────────────────────── */}
          <Section id="table" title="Table">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Rows</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow>
                  <TableCell className="font-medium">users</TableCell>
                  <TableCell className="text-muted-foreground">table</TableCell>
                  <TableCell className="tabular">12,408</TableCell>
                  <TableCell><Badge tone="success" dot>synced</Badge></TableCell>
                </TableRow>
                <TableRow>
                  <TableCell className="font-medium">orders</TableCell>
                  <TableCell className="text-muted-foreground">table</TableCell>
                  <TableCell className="tabular">488,221</TableCell>
                  <TableCell><Badge tone="warning" dot>pending</Badge></TableCell>
                </TableRow>
                <TableRow>
                  <TableCell className="font-medium">events_view</TableCell>
                  <TableCell className="text-muted-foreground">view</TableCell>
                  <TableCell className="tabular">—</TableCell>
                  <TableCell><Badge tone="brand" dot>live</Badge></TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </Section>

          {/* ── Spacer ───────────────────────────────────── */}
          <div className="h-10" />
        </div>
      </ScrollArea>

      {/* Demo Dialog */}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>shadcn Dialog</DialogTitle>
            <DialogDescription>
              Built on Radix Dialog. Press <Kbd>Esc</Kbd> to close.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2 py-1">
            <div className="space-y-1">
              <Label>Workspace name</Label>
              <Input placeholder="e.g. Marketing" />
            </div>
            <div className="space-y-1">
              <Label>Region</Label>
              <Select defaultValue="us-east-1">
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="us-east-1">us-east-1</SelectItem>
                  <SelectItem value="eu-west-1">eu-west-1</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>Cancel</Button>
            <Button onClick={() => { setOpen(false); toast.success('Workspace created.'); }}>Create</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
