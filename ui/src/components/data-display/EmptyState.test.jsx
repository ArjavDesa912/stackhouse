import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { EmptyState } from './EmptyState';

describe('EmptyState', () => {
  it('renders eyebrow, title, and description', () => {
    render(
      <EmptyState
        eyebrow="ONBOARD"
        title="Hello world"
        description="Get started with Stackhouse."
      />,
    );
    expect(screen.getByText('ONBOARD')).toBeInTheDocument();
    expect(screen.getByText('Hello world')).toBeInTheDocument();
    expect(screen.getByText('Get started with Stackhouse.')).toBeInTheDocument();
  });

  it('renders the action node', () => {
    render(
      <EmptyState
        title="Hi"
        action={<button type="button">Go</button>}
      />,
    );
    expect(screen.getByRole('button', { name: 'Go' })).toBeInTheDocument();
  });

  it('applies the default muted surface class', () => {
    const { container } = render(<EmptyState variant="default" title="x" />);
    const card = container.querySelector('[data-slot="card"]');
    expect(card).not.toBeNull();
    expect(card.className).toMatch(/bg-muted text-foreground/);
  });

  it('falls back to the muted default when no variant is provided', () => {
    const { container } = render(<EmptyState title="x" />);
    const card = container.querySelector('[data-slot="card"]');
    expect(card.className).toMatch(/bg-muted text-foreground/);
  });

  it('supports a plain background variant', () => {
    const { container } = render(<EmptyState variant="plain" title="x" />);
    const card = container.querySelector('[data-slot="card"]');
    expect(card.className).toMatch(/bg-background/);
  });

  it('omits illustration container when not provided', () => {
    const { container } = render(<EmptyState title="x" />);
    expect(container.querySelectorAll('.pointer-events-none').length).toBe(0);
  });
});
