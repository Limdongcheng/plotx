---
title: XPS
description: Import, charge-reference, process, fit, and export XPS spectra.
---

PlotX imports one VAMAS file as one XPS experiment. The Data browser keeps its
measurement-position → spectrum-region hierarchy, and every Survey or core
level is a stable curve field that can be plotted alone or overlaid across
positions. The default plot uses the first Survey, or the first readable
region when no Survey exists. Binding energy runs high-to-low from left to
right and intensity is shown in CPS.

## Supported input

- ISO 14976 VAMAS `.vms`: the first release supports `NORM` / `REGULAR` XPS
  blocks with a regular energy ruler. Unknown non-XPS blocks produce warnings
  while readable XPS blocks still import. Truncation, inconsistent point
  counts, non-finite ordinates, or no readable XPS block reject the file.
- Structured CasaXPS `.txt`: PlotX recognizes the eight-line CasaXPS header
  and preserves BE/CPS, background, envelope, components, line shapes, and
  original peak parameters. Generic two-column text remains a table import.

The native energy axis is always retained. A binding-energy axis is used
directly when supplied. For kinetic energy, PlotX derives `BE = hν - KE` only
when photon energy is present. A kinetic-only region can still be viewed and
exported, but charge correction and peak fitting are disabled.

VAMAS ordinate descriptors determine how the payload is decoded. PlotX does
not treat stored ordinate minima and maxima as spectrum points. A pulse-counted
intensity ordinate is retained as Counts and converted to CPS with the block
dwell time and scan count; an ordinate already labelled as a count rate is not
rescaled.

## Charge correction and processing

Select a C 1s region, keep the default reference at 284.8 eV or enter another
explicit value, then choose **Reference current C 1s**. PlotX locates the
smoothed C 1s maximum once and applies the resulting shift to every region at
that measurement position. Processing windows and background ranges follow the
same sampled points when the shift changes, while component centers remain
absolute chemical binding energies. The original arrays never change.

Each spectrum region has its own ordered, undoable processing recipe. Add an
energy window, Savitzky–Golay smoothing, or maximum normalization, then enable,
reorder, or delete steps in **Dataset tools → XPS**. The measurement-level
charge shift remains shared by every region at that position. A changed recipe
makes earlier PlotX fits stale; it does not rewrite their provenance.

## Background and peak fitting

The XPS workbench has **Acquisition**, **Background**, **Components**, and
**Diagnostics** tabs. Background is part of the fit invocation rather than the
processing recipe. Choose Linear, iterative Shirley, or Tougaard U2, then
preview and edit the fit window and low-/high-BE anchor bands before fitting.
The plot range-selection tool can fill any of those ranges; numerical inputs
remain available for exact values.

Tougaard U2 uses `K(T) = B T / (C + T²)²`, with editable defaults
`B = 3000 eV²` and `C = 1643 eV²`. It models an inelastic-loss tail; it is not
automatically preferable for every region, and its result remains sensitive to
window and anchor choices. PlotX applies this kernel as an anchored,
finite-window peak background; it is not a replacement for complete QUASES
depth-profile analysis.

Add and reorder components manually, explicitly choose the C 1s, N 1s, or O 1s
template, or copy a component as a linked component. Templates seed candidates
only; PlotX does not assert that a chemical assignment is correct. Stable
component identities keep energy offsets, shared widths, and area ratios
attached to the same component after reordering. Center, FWHM, and area can
each be free and bounded, fixed, or linked where applicable. Missing, self, and
cyclic links are rejected. Free areas have explicit editable lower and upper
bounds, and diagnostics plus data export report area-bound hits.

The default line shape is area-normalized GL(30) pseudo-Voigt. Fitting runs in
a cancellable background job. Results include the background, envelope,
components, residual, peak parameters, area fractions, R², RMSE, residual
lag-1, bound and correlation diagnostics, input hash, energy shift, and the
complete invocation. Numerical covariance is propagated through linked
parameters to standard errors and approximate 95% intervals. A singular local
matrix leaves those intervals unavailable instead of inventing certainty.

Optional wild residual Bootstrap runs 100–5000 replicates (500 by default) in
a cancellable job. Seed `0` derives a deterministic seed from the fit input;
enter another seed for an explicit repeatable sequence. PlotX stores the 2.5%,
50%, and 97.5% quantiles. Fewer than 80% converged replicates produces a clear
warning but does not discard the converged distribution. Bootstrap can be
computationally expensive, especially for highly coupled component sets.

R² and width/bound warnings are diagnostics, not a chemical-validity verdict.
CasaXPS fits remain `Imported`; they can be inspected and exported without
being represented as PlotX recomputations. Their original curves are overlaid
and included with processed-data exports only while the region has no enabled
processing steps; after windowing, smoothing, or normalization, PlotX hides
those incompatible raw-CPS curves but keeps their imported parameters.

## Scope

Survey spectra can be viewed, compared, annotated, and exported. PlotX does
not calculate Survey elemental atomic percentages in this release.

Use **Export Data…** for raw and processed axes/intensity, the background model,
window and anchors, background-subtracted intensity, envelope, residual,
components, parameter intervals, correlation diagnostics, and Bootstrap
intervals. PDF, SVG, PNG, TIFF, and JPEG remain available through figure
export. The `.plotx` project stores the experiment hierarchy, raw arrays,
active region, measurement charge shifts, per-region processing recipes and fit
workspaces, Imported results, and PlotX fit and Bootstrap provenance. PlotX fit curves are rebuilt from their
invocation and fitted parameters when a project loads; only Imported CasaXPS
results retain their original curve arrays.
