# Front-only plate ownership

## Status

Rejected

## Context

The default transfer model updates ownership twice: cell material mixing first reconstructs labels by
argmax, then Euler boundary-front transfer edits those labels. At alpha tick 400, runtime front
transfer is zero while label churn remains 1.37%, so material reconstruction is an independent and
non-kinematic ownership authority.

## Proposal

Use the previous labels as the sole input to Euler front advection. Material and crust fields follow
the committed front result and never reconstruct ownership. Keep the existing front accumulator and
topology validation for the first control, but treat plate-wide projection and transfer caps as
separate removal candidates rather than compensating for material churn.

This approximates an Eulerian front at mesh resolution. It preserves the rigid Euler velocity as the
motion source but does not yet provide sub-cell geometry or explicit topology events.

## Close when

Require zero label churn when runtime transfer is zero, alpha 40 shape no worse than model 0, and
lower temporal reversal with unchanged plate-direction persistence. Then run alpha 160/400 and
beta/gamma/delta 160 before considering it for preview.

## Outcome

Removing material argmax preserved one connected block per plate at alpha tick 40, but temporal
reversal remained 0.90. Removing plate-level projection raised response from 0.30 to 0.87 while
reversal remained 0.85 and complexity rose to 1.46. Signed pair-and-bucket accumulation reduced
reversal to 0.68 and kept complexity at 1.16, but boundary centroid straightness fell to 0.12.
Matching the bucket to level-4 mesh scale raised response but worsened reversal to 0.75 and
complexity to 1.69.

The model is rejected as an ownership replacement. A fixed spatial bucket is not a persistent
boundary component: material crosses bucket boundaries and unrelated arcs share or exchange
residuals. The controls remain useful evidence that stable Euler poles alone do not make a coherent
discrete front.
