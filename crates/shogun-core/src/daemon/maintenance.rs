//! Maintenance-domain notification mapping.

/// Convert newly overdue commitments into local, non-egress notifications.
pub(super) fn overdue_notifications(
    newly: &[shogun_memory::recompute::NewlyOverdue],
) -> Vec<shogun_agents::permission::Action> {
    use shogun_agents::permission::{Action, LocalAction};

    newly
        .iter()
        .map(|commitment| {
            Action::Local(LocalAction::ShowNotification {
                text: format!("Overdue: {}", commitment.description),
            })
        })
        .collect()
}
