/** @type {import('tailwindcss').Config} */
export default {
    content: [
        "./index.html",
        "./src/**/*.{js,ts,jsx,tsx}",
    ],
    theme: {
        extend: {
            fontFamily: {
                sans: ['Geist Variable', 'Inter', 'system-ui', 'sans-serif'],
                mono: ['Geist Mono Variable', 'ui-monospace', 'monospace'],
                display: ['Geist Variable', 'system-ui', 'sans-serif'],
                heading: ['Geist Variable', 'system-ui', 'sans-serif'],
            },
            colors: {
                background: 'var(--background)',
                foreground: 'var(--foreground)',
                card: {
                    DEFAULT: 'var(--card)',
                    foreground: 'var(--card-foreground)',
                },
                popover: {
                    DEFAULT: 'var(--popover)',
                    foreground: 'var(--popover-foreground)',
                },
                primary: {
                    DEFAULT: 'var(--primary)',
                    foreground: 'var(--primary-foreground)',
                },
                secondary: {
                    DEFAULT: 'var(--secondary)',
                    foreground: 'var(--secondary-foreground)',
                },
                muted: {
                    DEFAULT: 'var(--muted)',
                    foreground: 'var(--muted-foreground)',
                },
                accent: {
                    DEFAULT: 'var(--accent)',
                    foreground: 'var(--accent-foreground)',
                },
                destructive: {
                    DEFAULT: 'var(--destructive)',
                    foreground: 'var(--destructive-foreground)',
                },
                success: 'var(--success)',
                warning: 'var(--warning)',
                border: 'var(--border)',
                input: 'var(--input)',
                ring: 'var(--ring)',
            },
            borderRadius: {
                xs: '4px',
                sm: '6px',
                md: '10px',
                lg: '14px',
                xl: '20px',
                pill: '999px',
            },
            boxShadow: {
                'elev-1': 'var(--elev-1)',
                'elev-2': 'var(--elev-2)',
                'elev-3': 'var(--elev-3)',
                'elev-4': 'var(--elev-4)',
                'elev-5': 'var(--elev-5)',
                // legacy aliases retained so older components still resolve
                'ring-warm': 'var(--elev-1)',
                'ring-warm-deep': 'var(--elev-2)',
                'ring-dark': 'var(--elev-1)',
                whisper: 'var(--elev-3)',
                'whisper-lg': 'var(--elev-4)',
                'inset-warm': 'inset 0 0 0 1px var(--hairline)',
            },
            letterSpacing: {
                micro: '0.08em',
                tightest: '-0.04em',
            },
            transitionTimingFunction: {
                'out-stackhouse': 'cubic-bezier(0.22, 1, 0.36, 1)',
                'spring-stackhouse': 'cubic-bezier(0.34, 1.56, 0.64, 1)',
            },
        },
    },
    plugins: [],
}
