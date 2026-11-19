# WDDM qualification proposal — NOT approved admission policy

2026-11-19. Production GPU guard remains unchanged; dispatch HOLD. No GPU sampled for this proposal.

## What the existing evidence establishes

Fresh06 captured ten resident process rows with unavailable memory and encoder utilization 0. It did not capture encoder sessions, sampling period, or complete compute-engine activity. This cannot establish either active encoding or idle hardware. NVIDIA documents WDDM per-process memory as unavailable because Windows manages it. Residency is not an activity measurement.

## Option A: preserve strict admission

A naturally quiet logged-in period clears the existing guard only if its authorized exact-target query actually returns zero process rows and zero encoder activity, with the existing preclaim and subsequent guards passing. No process termination, exclusion list, or assumed quiet interval. A desktop may retain contexts indefinitely; waiting is not a promised solution. Any unavailable sample remains HOLD.

## Option B: separately qualify a coexistence policy

First authorize only a bounded read-only diagnostic, not a render or policy change. Pin the installed vendor NVML identity and map GPU UUID to the Windows adapter LUID. Obtain supported encoder session enumeration plus count and utilization/sampling period; count/list disagreement, insufficient buffer, unsupported or denied results are UNKNOWN. Obtain Windows scheduler engine evidence for that exact adapter with explicitly complete engine coverage, including compute, 3D, copy and video engines. Missing counters, unknown engine mapping, stale/non-overlapping windows or provider errors are UNKNOWN. A zero encoder session count alone is insufficient.

Only a qualified implementation can propose a finite observation window and freshness budget, justified by the actual telemetry sampling intervals. No invented thresholds in this proposal. Nonzero activity is contention, not a reason to whitelist its process. Independently retain one app worker claim and the repeated pre-work GPU guard. Do not claim either lock reserves the device against foreign applications.

Even complete zero activity samples cannot guarantee future exclusivity, prevent another application starting, or establish sufficient VRAM. DXGI QueryVideoMemoryInfo describes the calling process budget/usage, not global free memory. Any replacement therefore requires explicit acceptance of bounded coexistence/race risk and a separately reviewed resource policy. If zero foreign-context exclusivity remains the requirement, Option B cannot satisfy it; retain Option A/HOLD.

## Required synthetic qualification cases

| Evidence | Required result |
| --- | --- |
| Session exists, encoder utilization 0 | Contention/HOLD |
| Zero sessions, compute/3D/copy activity | Contention/HOLD |
| Zero sessions, incomplete engine coverage | UNKNOWN/HOLD |
| Encoder count/list disagree or buffer insufficient | UNKNOWN/HOLD; no automatic retry |
| Wrong UUID/LUID, stale samples, disjoint windows | UNKNOWN/HOLD |
| WDDM memory N/A, otherwise quiet | No exclusivity or memory-capacity claim |
| DXGI caller budget positive | Not proof of global free VRAM |
| All observations complete and zero | Candidate evidence for review only; no render authority |
| Activity begins after preclaim | Existing subsequent guard HOLD; never clear/retry claim |

These added cases are acceptance requirements, not executed hardware tests. Existing six pure encoder-interpreter tests remain the tested subset. Decision requested: retain zero-row requirement, or authorize a read-only telemetry qualification ticket toward an explicitly different coexistence requirement. Neither decision authorizes a render today.

Sources: https://docs.nvidia.com/deploy/nvidia-smi/ ; https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceQueries.html ; https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/ ; https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/nf-dxgi1_4-idxgiadapter3-queryvideomemoryinfo
