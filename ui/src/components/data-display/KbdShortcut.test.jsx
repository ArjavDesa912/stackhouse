import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Kbd, ShortcutGroup } from './KbdShortcut';

describe('Kbd', () => {
  it('renders a <kbd> element', () => {
    const { container } = render(<Kbd>K</Kbd>);
    const kbd = container.querySelector('kbd');
    expect(kbd).not.toBeNull();
    expect(kbd.textContent).toBe('K');
  });
});

describe('ShortcutGroup', () => {
  it('renders one Kbd per key in order', () => {
    const { container } = render(<ShortcutGroup keys={['⌘', 'K']} />);
    const kbds = container.querySelectorAll('kbd');
    expect(kbds.length).toBe(2);
    expect(kbds[0].textContent).toBe('⌘');
    expect(kbds[1].textContent).toBe('K');
  });

  it('renders nothing for an empty array', () => {
    const { container } = render(<ShortcutGroup keys={[]} />);
    expect(container.querySelectorAll('kbd').length).toBe(0);
  });
});

// Quick sanity: composed inline usage
describe('Kbd usage', () => {
  it('survives custom className', () => {
    render(<Kbd className="custom-y">Enter</Kbd>);
    const el = screen.getByText('Enter');
    expect(el.className).toMatch(/custom-y/);
  });
});
