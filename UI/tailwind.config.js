/** @type {import('tailwindcss').Config} */
module.exports = {
    darkMode: ["class"],
    content: ["./index.html", "./src/**/*.{ts,tsx,js,jsx}"],
  theme: {
  	extend: {
  		opacity: Object.fromEntries(Array.from({ length: 101 }, (_, i) => [i, `${i / 100}`])),
  		borderRadius: {
  			lg: 'var(--radius)',
  			md: 'calc(var(--radius) - 2px)',
  			sm: 'calc(var(--radius) - 4px)'
  		},
  		colors: {
  			background: 'hsl(var(--background))',
  			foreground: 'hsl(var(--foreground))',
  			card: {
  				DEFAULT: 'hsl(var(--card))',
  				foreground: 'hsl(var(--card-foreground))'
  			},
  			popover: {
  				DEFAULT: 'hsl(var(--popover))',
  				foreground: 'hsl(var(--popover-foreground))'
  			},
  			primary: {
  				DEFAULT: 'hsl(var(--primary))',
  				foreground: 'hsl(var(--primary-foreground))'
  			},
  			secondary: {
  				DEFAULT: 'hsl(var(--secondary))',
  				foreground: 'hsl(var(--secondary-foreground))'
  			},
  			muted: {
  				DEFAULT: 'hsl(var(--muted))',
  				foreground: 'hsl(var(--muted-foreground))'
  			},
  			accent: {
  				DEFAULT: 'hsl(var(--accent))',
  				foreground: 'hsl(var(--accent-foreground))'
  			},
  			destructive: {
  				DEFAULT: 'hsl(var(--destructive))',
  				foreground: 'hsl(var(--destructive-foreground))'
  			},
  			border: 'hsl(var(--border))',
  			input: 'hsl(var(--input))',
  			ring: 'hsl(var(--ring))',
  			chart: {
  				'1': 'hsl(var(--chart-1))',
  				'2': 'hsl(var(--chart-2))',
  				'3': 'hsl(var(--chart-3))',
  				'4': 'hsl(var(--chart-4))',
  				'5': 'hsl(var(--chart-5))'
  			},
  			sidebar: {
  				DEFAULT: 'hsl(var(--sidebar-background))',
  				foreground: 'hsl(var(--sidebar-foreground))',
  				primary: 'hsl(var(--sidebar-primary))',
  				'primary-foreground': 'hsl(var(--sidebar-primary-foreground))',
  				accent: 'hsl(var(--sidebar-accent))',
  				'accent-foreground': 'hsl(var(--sidebar-accent-foreground))',
  				border: 'hsl(var(--sidebar-border))',
  				ring: 'hsl(var(--sidebar-ring))'
  			},
  			onyx: {
  				bg: 'hsl(var(--background))',
  				surface: 'hsl(var(--surface-1))',
  				surface2: 'hsl(var(--surface-2))',
  				surface3: 'hsl(var(--surface-3))',
  				elevated: 'hsl(var(--elevated-1))',
  				elevated2: 'hsl(var(--elevated-2))',
  				line: 'hsl(var(--border))',
  				line2: 'hsl(var(--input))',
  				metallic: 'hsl(var(--metallic-1))',
  				metallic2: 'hsl(var(--metallic-2))',
  				text: 'hsl(var(--foreground))',
  				text2: 'hsl(var(--text-secondary))',
  				text3: 'hsl(var(--text-disabled))',
  				success: 'hsl(var(--success))',
  				warning: 'hsl(var(--warning))',
  				error: 'hsl(var(--error))',
  				info: 'hsl(var(--info))'
  			}
  		},
  		fontFamily: {
  			heading: ['var(--font-heading)'],
  			body: ['var(--font-body)'],
  			display: ['var(--font-display)'],
  			mono: ['var(--font-mono)']
  		},
  		keyframes: {
  			'accordion-down': {
  				from: {
  					height: '0'
  				},
  				to: {
  					height: 'var(--radix-accordion-content-height)'
  				}
  			},
  			'accordion-up': {
  				from: {
  					height: 'var(--radix-accordion-content-height)'
  				},
  				to: {
  					height: '0'
  				}
  			},
  			'onyx-sweep': {
  				'0%': { transform: 'rotate(0deg)' },
  				'100%': { transform: 'rotate(360deg)' }
  			},
  			'onyx-fade': {
  				'0%': { opacity: '0' },
  				'100%': { opacity: '1' }
  			},
  			'onyx-fade-up': {
  				'0%': { opacity: '0', transform: 'translateY(4px)' },
  				'100%': { opacity: '1', transform: 'translateY(0)' }
  			},
  			'onyx-scale-in': {
  				'0%': { opacity: '0', transform: 'scale(0.98)' },
  				'100%': { opacity: '1', transform: 'scale(1)' }
  			},
  			'onyx-pulse': {
  				'0%, 100%': { opacity: '0.35' },
  				'50%': { opacity: '1' }
  			}
  		},
  		animation: {
  			'accordion-down': 'accordion-down 0.2s ease-out',
  			'accordion-up': 'accordion-up 0.2s ease-out',
  			'onyx-sweep': 'onyx-sweep 1.6s linear infinite',
  			'onyx-fade': 'onyx-fade 0.18s ease-out both',
  			'onyx-fade-up': 'onyx-fade-up 0.22s ease-out both',
  			'onyx-scale-in': 'onyx-scale-in 0.16s ease-out both',
  			'onyx-pulse': 'onyx-pulse 1.8s ease-in-out infinite'
  		}
  	}
  },
  plugins: [require("tailwindcss-animate")],
}
