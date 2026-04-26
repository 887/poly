# Poly Server client plugin — English translations
# All keys MUST be prefixed with "plugin-poly-"

# --- Signup picker card ---
plugin-poly-signup-name = Poly Server
plugin-poly-signup-desc = Connect to a self-hosted Poly server.

# --- Signup page ---
plugin-poly-signup-title = Add Poly Server Account
plugin-poly-signup-description = Enter your server URL to connect. Poly will use your device identity key to sign in, and new Poly Server accounts also require an email address.
plugin-poly-signup-back = ← Choose Backend
plugin-poly-signup-url-label = Server URL
plugin-poly-signup-url-placeholder = http://127.0.0.1:7080

# --- Step 1: URL + Connect ---
plugin-poly-connect-btn = Connect

# --- Step 2: Sign-up form ---
plugin-poly-signup-no-account-desc = No account was found for your identity key on this server. Choose a username and email address to create a new account.
plugin-poly-existing-accounts-desc = This server already has one or more accounts linked to your identity key. Pick one to sign in, or create another account on this server.
plugin-poly-signup-another-account-desc = This identity key is already linked to other accounts on this server. Choose a username and email address to create an additional account.
plugin-poly-signup-username-label = Username
plugin-poly-signup-username-placeholder = alice
plugin-poly-signup-email-label = Email Address
plugin-poly-signup-email-placeholder = alice@example.com
plugin-poly-signup-displayname-label = Display Name
plugin-poly-signup-displayname-placeholder = Alice
plugin-poly-create-account-btn = Create Account
plugin-poly-create-another-account-btn = Create Another Account
plugin-poly-signup-back-btn = ← Back

# --- Shared ---
plugin-poly-signup-connecting = Connecting…
plugin-poly-signup-no-identity = Poly could not prepare an identity key for signup.

# --- Plugin settings page ---
plugin-poly-title = Poly Server
plugin-poly-settings-description = Configure connection options for the Poly Server backend.
plugin-poly-setting-websocket-label = Use WebSocket for real-time events
plugin-poly-setting-websocket-desc = When enabled, Poly opens a persistent WebSocket connection to receive messages and events instantly. Disable to fall back to HTTP polling. Requires reconnecting or restarting the app to take effect.

# --- Account profile tab (shown in per-account settings for Poly accounts) ---
plugin-poly-profile-title = Profile
plugin-poly-profile-section-desc = Manage your Poly Server profile information.
plugin-poly-profile-avatar-label = Profile Picture
plugin-poly-profile-display-name-label = Display Name
plugin-poly-profile-display-name-desc = Your display name is visible to other users on this server.
plugin-poly-profile-background-label = Server Banner / Background
plugin-poly-profile-background-desc = Banner image shown on your profile (coming soon).
plugin-poly-profile-status-label = Current Status
plugin-poly-profile-status-desc = Your availability as shown to other users. This is stored locally — server-side presence sync coming soon.
plugin-poly-profile-status-online = Online
plugin-poly-profile-status-away = Away
plugin-poly-profile-status-dnd = Do Not Disturb
plugin-poly-profile-status-appear-offline = Appear Offline
plugin-poly-profile-save = Save Profile
plugin-poly-profile-saved = Profile saved!
plugin-poly-profile-avatar-coming-soon = Avatar upload coming soon.
plugin-poly-profile-banner-coming-soon = Banner upload coming soon.

# --- Server context menu items ---
plugin-poly-menu-invite-people-label = Invite People
plugin-poly-menu-privacy-settings-label = Privacy Settings
plugin-poly-menu-edit-per-server-profile-label = Edit Server Profile
plugin-poly-menu-federation-settings-label = Federation Settings

# --- Account overview (Phase 2 — get_account_overview_view) ---
plugin-poly-overview-title = Your Servers
plugin-poly-overview-subtitle = All servers you have joined on this Poly Server account.

# --- Declarative settings sections (WP 3) ---
plugin-poly-setting-profile-label = Profile
plugin-poly-setting-nickname-label = Nickname
plugin-poly-setting-nickname-desc = Display this name instead of your account name in this server.
plugin-poly-setting-avatar-url-label = Avatar URL
plugin-poly-setting-avatar-url-desc = URL of the image to use as your avatar in this server. Leave empty to use your account avatar.
plugin-poly-setting-privacy-label = Privacy
plugin-poly-setting-allow-dms-from-server-members-label = Allow DMs from Members
plugin-poly-setting-allow-dms-from-server-members-desc = When enabled, other members of this server can send you direct messages.
plugin-poly-setting-federation-label = Federation
plugin-poly-setting-allow-federation-label = Allow Federation
plugin-poly-setting-allow-federation-desc = When enabled, this server can communicate with other federated Poly servers.
