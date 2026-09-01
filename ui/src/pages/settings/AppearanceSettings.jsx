import React, { useState } from 'react';
import { useTheme } from '@/components/ThemeProvider';
import { Button } from '@/components/ui/button';
import { Sun, Moon, Laptop } from 'lucide-react';
import { Row, SectionHeader, ThemeCard } from './components';

export default function AppearanceSettings() {
  const { theme, setTheme } = useTheme();
  const [density, setDensity] = useState('comfortable');
  const [font, setFont] = useState('inter');

  return (
    <section>
      <SectionHeader
        eyebrow="Appearance"
        title="Make it yours"
        description="Tune the surface, density and typography of your workspace. Preferences sync across devices."
      />

      <div className="mb-4">
        <p className="mb-2 text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">Theme</p>
        <div className="grid gap-2 sm:grid-cols-3">
          <ThemeCard value="light" label="Parchment" icon={Sun} active={theme === 'light'} onClick={() => setTheme('light')} />
          <ThemeCard value="dark" label="Near Black" icon={Moon} active={theme === 'dark'} onClick={() => setTheme('dark')} />
          <ThemeCard value="system" label="System" icon={Laptop} active={theme === 'system'} onClick={() => setTheme('system')} />
        </div>
      </div>

      <Row title="Density" description="Spacing and row heights across tables, menus and shelves.">
        <div className="inline-flex rounded-md border border-border bg-card p-0.5 shadow-none dark:shadow-none">
          {['compact', 'comfortable', 'spacious'].map((d) => (
            <button
              key={d}
              onClick={() => setDensity(d)}
              className={`rounded-md px-2 py-0.5 text-[11px] font-medium capitalize tracking-tight transition-colors ${
                density === d ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              {d}
            </button>
          ))}
        </div>
      </Row>

      <Row title="Interface font" description="UI sans-serif. Display titles always use Fraunces.">
        <select
          value={font}
          onChange={(e) => setFont(e.target.value)}
          className="h-8 rounded-md border border-border bg-card px-2 text-[12px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/40 sm:w-[240px]"
        >
          <option value="inter">Inter</option>
          <option value="geist">Geist</option>
          <option value="system">System default</option>
        </select>
      </Row>

      <Row title="Reduce motion" description="Disables non-essential transitions and parallax.">
        <Button variant="outline" size="sm" className="text-[12px]">Off</Button>
      </Row>
    </section>
  );
}
