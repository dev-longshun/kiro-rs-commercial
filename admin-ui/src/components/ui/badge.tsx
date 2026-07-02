import * as React from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const badgeVariants = cva(
  'inline-flex items-center border-[2px] border-border rounded-sm px-3 py-0.5 text-xs font-bold uppercase tracking-wider',
  {
    variants: {
      variant: {
        default: 'bg-foreground text-background',
        secondary: 'bg-background text-foreground',
        destructive: 'bg-nb-red text-nb-cream',
        success: 'bg-nb-green text-nb-cream',
        warning: 'bg-nb-yellow text-nb-ink',
        outline: 'bg-transparent text-foreground',
        info: 'bg-nb-blue text-nb-cream',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  )
}

export { Badge, badgeVariants }
