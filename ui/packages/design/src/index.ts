/**
 * @file The design system: primitive components every feature draws with.
 * Nothing here knows about dataframes, steps or tables. The colour and font
 * tokens these components use are defined in the shell's stylesheet, which
 * also imports `blocks.css`, the styles of the words "code" is written in.
 * The components under `shadcn/` are copied from shadcn/ui
 * (https://ui.shadcn.com), MIT License, Copyright (c) 2023 shadcn, and
 * drawn in the same tokens.
 */

export { Block, Equals, Seam, Word } from './Block';
export type { BlockProps, Look, Tone } from './Block';
export { Hint } from './Hint';
export type { HintLines, HintProps } from './Hint';
export { OfferList } from './OfferList';
export type { ListAction, Offer, OfferGroup, OfferListProps } from './OfferList';
export { Panel } from './Panel';
export type { PanelProps } from './Panel';
export { Peek } from './Peek';
export type { PeekProps } from './Peek';
export { Piece } from './Piece';
export type { PieceActions, PieceProps, SideAction } from './Piece';
export { Popover } from './Popover';
export type { PopoverProps } from './Popover';
export { ManySlot, Slot } from './Slot';
export type { Entry, ManySlotProps, SlotProps } from './Slot';
export { ToggleButton } from './ToggleButton';
export type { ToggleButtonProps } from './ToggleButton';
export { TypedBlock } from './TypedBlock';
export type { TypedBlockProps, TypedKind } from './TypedBlock';
export { cx } from './cx';

// Components copied from shadcn/ui, with the changes their files note.
export {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSkeleton,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from './shadcn/sidebar';
export { Collapsible, CollapsibleContent, CollapsibleTrigger } from './shadcn/collapsible';
export { Skeleton } from './shadcn/skeleton';
export { cn } from './shadcn/cn';
