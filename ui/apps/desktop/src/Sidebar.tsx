/**
 * @file The side panel along the window's left edge, drawn with shadcn/ui's
 * sidebar. Its top row is left empty for the window's own controls and the
 * button that shows and hides it, which the shell draws above the panel in
 * one place, so they stay still while the panel slides. ⌘B, or Ctrl+B, shows
 * and hides it too.
 */

import {
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
  cx,
} from '@dageist/design';

/**
 * The room left at the left of the top row for the window's own controls.
 * macOS draws them over the page where `trafficLightPosition` in
 * crates/dageist-tauri/tauri.conf.json puts them: 20 from the left, and with
 * a y of 28, which seen on screen puts them level with the middle of a row 52
 * high. They end about 74 from the left, and the toggle stands clear of them.
 * A browser has no such controls, and the shell shows the application's name
 * there instead.
 */
export const CONTROLS_ROOM = 'pl-[88px]';

/** The height of the top row, whose middle the window's controls are level with. */
export const TOP_ROW = 'h-[52px]';

/** A workspace the side panel lists. */
export interface SidebarEntry<Id extends string> {
  readonly id: Id;
  readonly label: string;
  /** A short glyph that stands for the workspace. */
  readonly mark: string;
}

/** What the side panel shows. */
export interface AppSidebarProps<Id extends string> {
  readonly workspaces: ReadonlyArray<SidebarEntry<Id>>;
  readonly current: Id;
  readonly onShow: (id: Id) => void;
  /** What is said about the host, at the foot of the panel. */
  readonly status: React.ReactNode;
}

/** The number of stand-ins in the list of nodes, which the panel does not have yet. */
const NODE_STAND_INS = ['first', 'second', 'third', 'fourth'];

/**
 * Draws the side panel: the window's controls and the toggle, the
 * workspaces, a list of the graph's nodes still to come, and the host's
 * status at the foot. It must stand inside a `SidebarProvider`, which holds
 * whether it is shown.
 */
export function AppSidebar<Id extends string>({
  workspaces,
  current,
  onShow,
  status,
}: AppSidebarProps<Id>): React.JSX.Element {
  return (
    <Sidebar collapsible="offcanvas">
      {/*
       * The top row is left empty for the window's controls and the toggle,
       * which the shell draws above the panel so they do not slide with it.
       * The row still moves the window when dragged.
       */}
      <SidebarHeader data-tauri-drag-region className={cx('shrink-0 p-0', TOP_ROW)} />
      <SidebarContent>
        <SidebarGroup>
          <SidebarMenu>
            {workspaces.map((entry) => (
              <SidebarMenuItem key={entry.id}>
                <SidebarMenuButton
                  isActive={entry.id === current}
                  onClick={() => {
                    onShow(entry.id);
                  }}
                >
                  <span aria-hidden="true" className="w-4 text-center text-accent">
                    {entry.mark}
                  </span>
                  <span>{entry.label}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroup>
        <SidebarGroup>
          <SidebarGroupLabel>Nodes</SidebarGroupLabel>
          <SidebarMenu>
            <span className="sr-only">Not loaded</span>
            {NODE_STAND_INS.map((key) => (
              <SidebarMenuItem key={key}>
                <SidebarMenuSkeleton />
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter className="border-t border-sidebar-border px-4 text-xs text-muted">
        {status}
      </SidebarFooter>
    </Sidebar>
  );
}
