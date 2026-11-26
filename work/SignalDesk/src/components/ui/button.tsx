import * as React from 'react';
import {Slot} from '@radix-ui/react-slot';
import {cva, type VariantProps} from 'class-variance-authority';
import {twMerge} from 'tailwind-merge';
const styles=cva('btn',{variants:{variant:{primary:'primary',outline:'outline',ghost:'ghost'}},defaultVariants:{variant:'outline'}});
type Props=React.ButtonHTMLAttributes<HTMLButtonElement>&VariantProps<typeof styles>&{asChild?:boolean};
export const Button=React.forwardRef<HTMLButtonElement,Props>(({asChild,variant,className,...props},ref)=>{
 const Component=asChild?Slot:'button'; return <Component ref={ref} className={twMerge(styles({variant}),className)} {...props}/>;
});
Button.displayName='Button';
