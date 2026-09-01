import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Badge } from './Badge';

describe('Badge', () => {
  it('renders neutral tone by default', () => {
    render(<Badge>hello</Badge>);
    const el = screen.getByText('hello');
    expect(el).toBeInTheDocument();
    expect(el.className).toMatch(/bg-muted/);
  });

  it('applies tone classes', () => {
    render(<Badge tone="success">synced</Badge>);
    expect(screen.getByText('synced').className).toMatch(/text-success/);
  });

  it('renders a dot when requested', () => {
    const { container } = render(<Badge tone="warning" dot>pending</Badge>);
    const dot = container.querySelector('span > span');
    expect(dot).not.toBeNull();
    expect(dot.className).toMatch(/bg-warning/);
  });

  it('merges custom className', () => {
    render(<Badge className="custom-x">v1</Badge>);
    expect(screen.getByText('v1').className).toMatch(/custom-x/);
  });
});
