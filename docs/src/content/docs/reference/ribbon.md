---
title: The Ribbon
description: Every Ribbon tab and group, and how the Ribbon adapts to window width.
---

The Ribbon is the command strip above the canvas, organized into task tabs
that follow the pipeline: **Data → Process → Analyze → Figure → Arrange**,
with **View** last. It is a shortcut surface — everything on it is also in
the menus or the command palette — so this page lists the vocabulary you see
on screen rather than every command.

Hovering any Ribbon button shows the full command name and its shortcut. A
grayed button explains, in the same tooltip, what to do before it becomes
available.

## Tabs and groups

- **Data** — **Import** (open files, folders, tables, and images),
  **Build** (new tables, stacking), **Export** (data export).
- **Process** — **Processing** (the pipeline steps: apodization, zero fill,
  phase, baseline, reference, and friends), **Correct** (the interactive
  manual-phase tool), **Transform** (arithmetic, spectrum alignment, CRAFT),
  **Recipes** (processing templates).
- **Analyze** — **Range** (the analysis range every fit reads), then the
  groups your active dataset supports: **XPS** (the XPS workbench pages),
  **Extract** (mass spectra), **Regions**, **Peaks**, **Review** (2D symmetry),
  **Overlay** (trace alignment), **Peak Fit**, **Curve Fit**, **Statistics**,
  and **Interpret** (integrals and multiplets).
- **Figure** — **Create** (canvas presets), **Chart**, **Data**, **Style**,
  **Canvas**, **Output** (copy and export).
- **Arrange** — **Layout**, **Align**, **Distribute**, **Order**, **Guides**,
  **Annotate**, **Object**, **Canvas**.
- **View** — **Navigate** (zooming and fitting), **Display** (side bars,
  layout grid, present mode, preferences).

Groups whose whole subject cannot apply to the active dataset kind are hidden;
anything temporarily unavailable stays visible and disabled with a reason.

## Width behavior

The Ribbon measures the active tab's content against the window:

- When everything fits, buttons render as icon-over-label tiles.
- When it does not, buttons drop to a compact row that keeps each label next
  to its icon, and whole low-priority groups move into the **More** menu.
- The command area never hides by itself. **Collapse ribbon** (the caret at
  the right end of the task row) puts it away and brings it back.

Mutually exclusive choices, such as the spacing basis and minimum spacing on
the **Arrange** tab, render as one segmented control with a single selected
segment. Buttons that start a computation, such as **Run Peak Fit**, use the
filled accent style — the same as a task card's Run button.
