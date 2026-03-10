# Knowledge: Poly backend disable fix 2026-03-10

*Last Updated: 2026-03-10T22:45:08.190817665+00:00*

---

Toggling off a native backend (poly/others) in Plugins settings now properly disconnects sessions. Added take_accounts_by_backend() sync method to ClientManager for two-phase disconnect. Poly backend available flag now uses cfg!(feature = "server"). active_account_ids() now returns sessions.keys() instead of backends.keys() so offline accounts appear.
