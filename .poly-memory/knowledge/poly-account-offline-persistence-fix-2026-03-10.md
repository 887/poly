# Knowledge: Poly account offline persistence fix 2026-03-10

*Last Updated: 2026-03-10T22:45:12.779344210+00:00*

---

When restore_poly_accounts fails to reconnect (server offline), it now calls register_offline_session() which adds session to client_manager.sessions (not backends) with ConnectionStatus::Disconnected and AccountPresence::Offline. Account icon appears in favorites bar with offline indicator. active_account_ids() uses sessions.keys() so offline accounts are visible.
