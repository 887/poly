# Poly — English (en) main translations
# Project Fluent (.ftl) format

# Application
app-title = Poly
electron-window-minimize = Minimize
electron-window-maximize = Maximize or restore
electron-window-close = Close window
app-description = Multi-platform messenger client
wasm-crash-title = Poly hit a browser crash
wasm-crash-description = The current page crashed or threw an unhandled browser/WASM error. The UI below this overlay is no longer trustworthy.
wasm-crash-details-label = Crash type
wasm-crash-location-label = Source location
wasm-crash-path-label = Route
wasm-crash-reload-action = Reload Poly
wasm-crash-kind-panic = Rust panic
wasm-crash-kind-window-error = Browser error event
wasm-crash-kind-unhandled-rejection = Unhandled promise rejection
wasm-crash-kind-unknown = Unknown crash
wasm-crash-generic-message = No crash details were provided by the browser.
wasm-crash-window-error-fallback = The browser reported a global error event without a message.
wasm-crash-rejection-fallback = A promise was rejected without a readable error message.

# Navigation
nav-dms = Direct Messages
nav-friends = Friends
nav-notifications = Notifications
nav-settings = Settings
nav-search = Search
nav-servers = Servers
nav-demo = Toggle Demo Client
nav-demo-active = Demo Client Active

# Setup Wizard
setup-welcome-title = Welcome to Poly
setup-welcome-tagline = Connect all your socials. Let AI help you stay connected. Your relationships, amplified.
setup-card-connect-title = All your socials, one place
setup-card-connect-body = Chat platforms, federated networks, team workspaces, open forums, link aggregators — every conversation in one unified sidebar. No more tab-switching.
setup-card-ai-title = AI that socializes with you
setup-card-ai-body = Poly runs an MCP server so any AI — Claude, ChatGPT, your own — can read your chats, draft replies, and remind you to reach out. Agentic socializing, finally real.
setup-card-byoa-title = Bring your own AI
setup-card-byoa-body = No vendor lock-in. Connect any AI provider via MCP — keys stay on your machine, conversations stay private. You vibe, your AI assists.
setup-get-started = Get Started
setup-welcome-description = A unified messenger for all your chat platforms.
setup-generating-keys = Generating your identity keys...
setup-your-account-id = Your Account ID
setup-account-id-description = This is your unique identifier. Share it with friends to connect.
setup-recovery-phrase = Recovery Phrase
setup-recovery-phrase-description = Write down these words and store them safely. You'll need them to recover your account.
setup-recovery-warning = If you lose your recovery phrase, you will permanently lose access to your account.
setup-copy-phrase = Copy Phrase
setup-export-phrase = Export to File
setup-confirm-phrase = Confirm Recovery Phrase
setup-confirm-description = Enter the words from your recovery phrase to confirm you've saved them.
setup-continue = Continue
setup-skip-confirmation = Skip Confirmation
setup-complete = Setup Complete
setup-complete-description = Your identity has been created. Add messenger accounts in Settings.
setup-go-to-app = Go to Poly

# Chat
chat-type-message = Type a message...
chat-send = Send
chat-typing = { $user } is typing...
chat-typing-multiple = { $count } people are typing...
chat-no-messages = No messages yet. Start the conversation!
chat-load-more = Load more messages
chat-edited = (edited)
chat-loading = Loading messages...
chat-select-conversation = Select a conversation
chat-loading-earlier = Loading older messages...
chat-unread-banner = { $count } new messages since { $time } on { $date }
chat-unread-divider = New
chat-jump-to-present = Jump to Present
chat-viewing-older-messages = You're Viewing Older Messages
chat-readonly-notice = This channel is read-only

# Channels
channel-text = Text Channel
channel-voice = Voice Channel
channel-video = Video Channel

# Users / Status
user-online = Online
user-idle = Idle
user-dnd = Do Not Disturb
user-invisible = Invisible
user-offline = Offline
user-away = Away
user-appear-offline = Appear Offline
user-members = Members
user-no-members = No members

# Account bar — avatar corner badges
account-profile-click-hint = Click to view your profile
account-conn-connected = Connected
account-conn-connecting = Connecting…
account-conn-disconnected = Offline
account-conn-error = Connection Error

# Status picker popup
status-picker-title = Set Status

# Member list filter
member-filter-placeholder = Search members…
member-filter-tooltip = Search members
member-filter-no-results = No members match that search.

# User profile modal
user-profile-more-options = More options
user-profile-message = Message
user-profile-call = Call
user-profile-video = Video
user-profile-add-to-call = Add to Call
user-profile-add-video-to-call = Add Video to Call
user-profile-note = Note
user-profile-note-placeholder = Click to add a note
user-profile-open = View profile

# Notifications
notifications-title = Notifications
notifications-empty = No new notifications
notifications-mark-read = Mark as Read
notifications-dismiss = Dismiss
notifications-reconnect = Sign In Again
notifications-reauth-preview = Your session expired. Sign in again to reconnect.
notifications-mention = { $user } mentioned you in { $channel }
notifications-friend-request = { $user } sent you a friend request
notifications-server-invite = You've been invited to { $server }
notifications-accept = Accept
notifications-decline = Decline
notifications-deny = Deny
notifications-join-voice = Join
notifications-voice-invite = { $user } invited you to { $channel }
notifications-show-all = Show all notifications
notifications-show-unread = Show unread only

# Settings
settings-title = Settings
settings-accounts = Accounts
settings-accounts-description = Manage your messenger accounts
settings-add-account = Add Account
settings-remove-account = Remove Account
settings-no-accounts = No accounts connected. Add an account to get started.
settings-account-settings-link = Account Settings
account-switch = Switch Account
account-settings = Account Settings
settings-account-settings = Account Settings

# Signup flow — backend picker
signup-picker-title = Add Account
signup-picker-description = Choose which type of account to add.
signup-picker-back = ← Back to Settings
signup-stub-back = ← Choose Backend

# ── Create server / channel ──────────────────────────────────────────────────
create-server-btn = Create Server
create-server-placeholder = Server name…
create-server-submit = Create
create-server-cancel = Cancel
create-server-creating = Creating…
create-server-page-title = Create a Server
create-server-page-subtitle = Give your server a name to get started. You can always change it later.
create-server-page-label = Server Name
channel-list-text-channels = Text Channels
create-channel-btn = New Channel
create-channel-page-title = Create a Channel
create-channel-page-subtitle = Give your channel a name. You can always change it later.
create-channel-page-label = Channel Name
create-channel-placeholder = Channel name…
create-channel-submit = Create
create-channel-cancel = Cancel
create-channel-creating = Creating…

settings-backup = Backup Servers
settings-backup-description = Configure encrypted backup sync servers
settings-add-backup = Add Backup Server
settings-identity = Identity
settings-identity-description = Your device identity, recovery phrase, and where this identity is used
settings-your-id = Your Account ID
settings-export-recovery = Export Recovery Phrase
settings-theme = Theme
settings-theme-description = Customize colors, themes, and appearance
settings-media = Media
settings-media-description = Configure GIF providers and future rich media integrations
settings-media-description-tabs = Configure GIF providers. Enable a provider to make it available in the chat GIF picker. Enabling a provider also makes it the active one.
settings-media-active-hint = The enabled providers appear as tabs in the GIF picker when composing a message.
settings-media-active-provider = Active GIF Provider
settings-media-api-key = API Key
settings-media-api-key-placeholder = Paste provider API key
settings-media-provider-klippy = Klippy
settings-media-provider-giphy = Giphy
settings-media-provider-imgur = Imgur
settings-media-status-configured = Configured
settings-media-status-not-setup = Not setup
settings-theme-preset = Theme Preset
settings-theme-custom-css = Custom CSS
settings-theme-import = Import Theme
settings-theme-export = Export Theme
settings-color-mode = Color Mode
settings-color-overrides = Color Customization
settings-color-hint = Enable to override individual colors from the preset. Disable to revert to the preset theme.
settings-reset-colors = Reset Colors
settings-theme-apply-css = Apply CSS
settings-css-hint = Uncomment any variable to override the theme preset. The toggle enables/disables these CSS overrides.
settings-css-reset-template = Reset Template
settings-translation = Translation
settings-translation-description = Translate messages on the fly without sending text to external servers.
settings-translation-browser-title = Browser Built-in
settings-translation-browser-body = Uses your browser's on-device translation models. No download required — works automatically when available.
settings-translation-bergamot-title = Bergamot (On-device)
settings-translation-bergamot-body-needed = Your browser doesn't have built-in translation. Download the Bergamot engine to enable on-device translation — no API keys, no external servers.
settings-translation-bergamot-body-optional = Browser translation is available, but you can also install Bergamot for offline use or as a fallback.
settings-translation-download-engine = Download engine
settings-translation-checking = Checking…
settings-translation-available = Available
settings-translation-not-available = Not available
settings-translation-not-installed = Not installed
settings-translation-coming-soon = Full Bergamot support with per-language model downloads coming in a future update.
settings-language = Language
settings-language-description = Choose your preferred language
settings-appearance = Appearance
settings-appearance-description = Dark mode, light mode, and display options
settings-dark-mode = Dark Mode
settings-light-mode = Light Mode
settings-follow-device = Follow Device Preference
settings-layout = Layout
settings-layout-description = Layout behavior and mirroring across desktop and mobile shells
settings-general = General
settings-general-description = Reset local app data or fully nuke state for clean re-testing
settings-layout-mode = Layout mode
settings-layout-mode-description = Choose whether Poly should auto-detect mobile by width, auto-detect by portrait orientation, or always force desktop/mobile. URL query overrides like ?layout=mobile or ?layout=desktop take priority while present.
settings-layout-auto-width = Auto (width ≤ 640px)
settings-layout-auto-portrait = Auto (portrait)
settings-layout-force-desktop = Force desktop
settings-layout-force-mobile = Force mobile
settings-mirror-menu-layout = Mirror app menus / wings
settings-mirror-menu-layout-description = Swap the left and right app wings across desktop and mobile, including the sidebar order and mobile header wing buttons.
settings-mirror-chat-messages = Mirror chat message rows
settings-mirror-chat-messages-description = Put message avatars / gutters on the right while keeping the text readable.
settings-force-mobile-layout = Force mobile layout
settings-force-mobile-layout-description = Use the mobile shell even above 640px. Leave this off to use the desktop shell until the window is naturally narrow.
settings-reset-description = Reset app data for a fresh start, or fully nuke all local state for clean re-testing.
settings-reset-app = Reset App Data
settings-nuke-app = NUKE App State
settings-reset-error-no-storage = Storage is not ready yet
settings-reset-error-failed = Failed to reset app data
settings-nuke-error-failed = Failed to nuke app state
settings-reset-error-reload = Reset succeeded, but reload failed

# Diagnostics Settings
settings-diagnostics = Diagnostics
settings-diagnostics-title = Diagnostics
settings-diagnostics-description = Connection health, account status, and storage information.
settings-diagnostics-demo-active = Demo mode active
settings-diagnostics-active-accounts = Active accounts
settings-diagnostics-accounts-title = Account Status
settings-diagnostics-col-account = Account
settings-diagnostics-col-connection = Connection
settings-diagnostics-col-presence = Presence
settings-diagnostics-no-accounts = No accounts are currently active.

# Settings - MCP Server
settings-mcp = MCP Server
settings-mcp-description = Poly runs a built-in MCP (Model Context Protocol) server so any AI — Claude Desktop, ChatGPT, or your own — can connect to all your chat backends and act on your behalf.
settings-mcp-enable = Enable MCP server
settings-mcp-port = Port
settings-mcp-status-running = Running on port
settings-mcp-status-stopped = Not running
settings-mcp-status-web = Available in the desktop app
settings-mcp-restart = Restart
settings-mcp-config-title = Add Poly to your AI client
settings-mcp-config-description = Paste this into your AI client's MCP config (e.g. Claude Desktop → Settings → Developer → Edit Config):
settings-mcp-link-docs = How to connect local MCP servers →
settings-mcp-link-wikipedia = What is MCP? (Wikipedia) →
settings-mcp-links-title = Learn more

# Demo Settings
settings-demo = Demo
settings-demo-description = Manage the built-in demo data client. When enabled, Poly loads sample accounts with servers, channels, and conversations so you can explore the app.
settings-demo-toggle = Enable Demo Data

# Plugin Manager
settings-plugins = Plugins
settings-plugins-description = Every Poly messenger backend is a WASM plugin. Built-in plugins ship with the app; sideloaded plugins are added by you at runtime. Accounts are sessions created by those plugins.
plugins-builtin-title = Built-in WASM Plugins
plugins-builtin-description = Plugins bundled with Poly. Updated alongside the app. Backends marked "not in this build" are not compiled into this version — enabling them here saves your preference for when they become available, or add them as a sideloaded plugin below.
plugins-loaded-count = Active backends
plugins-none-loaded = No sideloaded plugins yet. Add a plugin URL or upload a .wasm file below to get started.
plugins-status-disconnected = Disconnected
plugins-status-connecting = Connecting…
plugins-status-connected = Connected
plugins-status-error = Error
plugins-type-builtin = Built-in
plugins-type-sideloaded = Sideloaded
plugins-type-bundled = Bundled
plugins-not-compiled = not in this build
plugins-active-accounts = Active accounts
plugins-sideloaded-title = Sideloaded WASM Plugins
plugins-sideloaded-description = User-installed plugins. Add via the form below: paste a URL or upload a local .wasm file. Sideloaded plugins do not auto-update with Poly — re-add them to upgrade.
plugins-add-wasm-title = Add Plugin
plugins-add-wasm-description = Enter the base URL of a WASM plugin. The WIT version will be appended automatically so you always get a compatible build.
plugins-url-placeholder = https://plugins.example.com/matrix.wasm
plugins-add-btn = Add Plugin
plugins-url-required = Please enter a plugin URL
plugins-install-from-url = From URL
plugins-install-from-file = From File
plugins-add-file-description = Select a local .wasm file to install as a plugin. The plugin can optionally contain its own update URL in its metadata.
plugins-file-hint = The plugin will be registered locally. Reload the app to activate it.
plugins-remove = Remove
plugins-remove-confirm = Remove this plugin?
plugins-remove-yes = Yes, remove
plugins-remove-cancel = Cancel
plugins-wit-hint = WIT interface version

# Plugin capabilities panel (shown when a plugin row is expanded)
plugins-capabilities-title = Capabilities
plugins-capabilities-shape = Backend shape
plugins-capabilities-flags = Feature flags
plugins-capabilities-terminology = Terminology
plugins-capabilities-show = Show capabilities
plugins-capabilities-hide = Hide capabilities
plugins-capabilities-container = Container noun
plugins-capabilities-layout = Layout
plugins-capabilities-layout-forum = Forum layout
plugins-capabilities-layout-chat = Chat layout
plugins-flag-supported = Supported
plugins-flag-unsupported = Not supported

cap-label-messaging = Messaging
cap-label-dms = Direct messages
cap-label-friends = Friends
cap-label-notifications = Notifications
cap-label-voice = Voice & video

cap-value-messaging-none = None
cap-value-messaging-readonly = Read-only feed
cap-value-messaging-full = Full (read & write)
cap-value-dms-none = Not supported
cap-value-dms-user = User-to-user DMs
cap-value-friends-none = No friends list
cap-value-friends-full = Friends list
cap-value-notifications-none = None
cap-value-notifications-inbox = Inbox (replies, mentions)
cap-value-notifications-activity = Activity stream
cap-value-voice-none = No voice
cap-value-voice-full = Voice & video channels

cap-flag-presence = Presence
cap-flag-typing = Typing indicators
cap-flag-reactions = Reactions
cap-flag-search = Message search
cap-flag-attachments = Attachments
cap-flag-create-server = Create container
cap-flag-create-channel = Create channel

# Plugin Settings
settings-plugin-settings = Plugin Settings
# Label shown in the nav sidebar and in the scroll divider before plugin-contributed sections
settings-plugins-section-divider = Plugin-Provided Settings
# Group header in the settings sidebar nav separating built-in sections from plugin pages
settings-plugin-settings-nav-header = Plugin Settings
# Small badge label shown on plugin-sourced settings headings
settings-plugins-badge = Plugin
plugin-settings-nav-title = Backend Settings
plugin-settings-none = No backends with settings are loaded. Enable demo data or connect an account.
plugin-settings-generic-description = This backend does not have custom settings yet. Settings will appear here when the plugin supports them.
# Note: plugin-demo-* strings are loaded from the demo plugin's own FTL bundle.

# Backup Server Settings
settings-backup-add-server = Add Server
settings-backup-url-placeholder = http://127.0.0.1:8080
settings-backup-url-label = Server URL
settings-backup-label-label = Server Name
settings-backup-passphrase-label = Server Passphrase
settings-backup-connect = Connect
settings-backup-connecting = Connecting...
settings-backup-cancel = Cancel
settings-backup-status-unknown = Unknown
settings-backup-status-connected = Connected
settings-backup-status-auth-required = Auth Required
settings-backup-status-unreachable = Unreachable
settings-backup-status-syncing = Syncing...
settings-backup-sync-now = Sync Now
settings-backup-reauth = Re-authenticate
settings-backup-remove = Remove
settings-backup-last-synced = Last synced: { $time }
settings-backup-never-synced = Never synced
settings-backup-enabled = Enabled
settings-backup-auth-success = Connected!
settings-backup-auth-failed = Authentication failed
settings-backup-no-servers = No backup servers configured.
settings-backup-wizard-step1 = Server URL
settings-backup-wizard-step2 = Connect
settings-backup-step1-hint = Enter the URL of your Poly backup server
settings-backup-step2-hint = Set a name and enter credentials to complete setup
settings-backup-check-btn = Check Connection
settings-backup-checking = Checking…
settings-backup-continue = Continue
settings-backup-back = Back
settings-backup-finish = Finish Setup
settings-backup-url-empty = Please enter a server URL
settings-backup-password-required = 🔒 Password required
settings-backup-no-password-required = ✓ No password required
settings-backup-server-full = Server is at full capacity — registrations disabled

# Identity Settings
settings-identity-your-id-label = Your Poly Account ID
settings-identity-copy-id = Copy ID
settings-identity-show-phrase = Show Recovery Phrase
settings-identity-phrase-modal-title = Your Recovery Phrase
settings-identity-phrase-warning = Keep this phrase secret. Anyone who has it can access your account.
settings-identity-copy-all = Copy All Words
settings-identity-close = Close
settings-identity-no-identity = Identity not yet generated. Complete the setup wizard first.
settings-identity-create-btn = Create Identity
settings-identity-creating = Creating…
settings-identity-purpose = This identity is the key material Poly uses on your behalf:
settings-identity-purpose-poly = Poly Servers use it for key-based sign-in and end-to-end encrypted features.
settings-identity-purpose-backup = Backup Servers use it to derive encryption keys and authenticate encrypted sync.
settings-identity-backup-servers = Backup Servers
settings-identity-backup-servers-description = This identity is used for authentication on the following backup servers.
settings-identity-poly-accounts = Poly Server Accounts
settings-identity-poly-accounts-description = This identity is used for the following accounts on self-hosted Poly servers.
settings-identity-no-servers = No backup servers configured yet.
settings-identity-no-poly-accounts = No Poly server accounts.
settings-identity-status-active = Active
settings-identity-status-disabled = Disabled
settings-identity-delete = Delete Identity
settings-identity-delete-confirm-title = Delete Identity?
settings-identity-delete-confirm-message = This will permanently remove this identity key. Make sure you have the recovery phrase backed up or you won't be able to recover access!
settings-identity-delete-confirm = Yes, Delete
settings-identity-cancel = Cancel

# Theme Presets
theme-blue = Blue
theme-purple = Purple
theme-red = Red
theme-green = Green
theme-monotone = Monotone

# Backends
backend-stoat = Stoat
backend-matrix = Matrix
backend-discord = Discord
backend-teams = Teams
backend-demo = Demo

# Common Actions
action-save = Save
action-cancel = Cancel
action-delete = Delete
action-edit = Edit
action-close = Close
action-more = More
chat-replying-to = Replying to { $name }
action-search = Search
action-copy = Copy
action-back = Back
action-confirm = Confirm
action-clear = Clear
action-download = Download
action-open-in-browser = Open in browser
zoom-in = Zoom in
zoom-out = Zoom out
mobile-nav-open = Open navigation menu
mobile-nav-close = Close navigation menu

media-viewer-unavailable-title = Media unavailable
media-viewer-unavailable-body = This media could not be loaded from the current chat state.

# Errors
error-generic = Something went wrong. Please try again.
error-network = Network error. Check your connection.
error-auth-failed = Authentication failed. Please check your credentials.
error-not-found = Not found.

# Voice / Video
voice-connected = Voice Connected
voice-join-voice = Join Voice
voice-join-video = Join Video
voice-direct-call = Direct Call
voice-group-call = Group Call
voice-swap-held-call = Swap to Held Call
voice-disconnect = Disconnect
voice-muted = Muted
voice-deafened = Deafened
voice-streaming = Sharing Screen
voice-video-on = Camera On
voice-mute = Mute
voice-unmute = Unmute
voice-deafen = Deafen
voice-undeafen = Undeafen
voice-no-channel = No channel selected
voice-no-one-here = No one is here yet
voice-be-first = Be the first to join!
voice-watching-screen = Watching screen share
voice-in-channel = in channel
voice-in-call = in call
voice-go-to-channel = Go to channel
voice-go-to-conversation = Go to conversation
direct-call-calling = Calling…
direct-call-calling-video = Starting video call…
direct-call-adding = Adding to call…
direct-call-adding-video = Adding video to call…
direct-call-awaiting-join = Waiting for the call to connect
direct-call-ringing = Ringing… tap × to cancel
direct-call-cancel = Cancel Call
voice-mute-mic = Mute microphone
voice-unmute-mic = Unmute microphone
voice-camera = Toggle Camera
voice-screen-share = Share Screen
voice-activity = Share Activity
voice-voiceboard = Voiceboard
voice-signal-quality = Signal Quality
voice-stop-camera = Stop Camera
voice-stop-share = Stop Sharing
voice-camera-preview = Camera Preview
voice-screen-sharing = Screen Share Preview
voice-audio-settings = Voice & Audio Settings
voice-mic-device = Input Device (Microphone)
voice-speaker-device = Output Device (Speaker)
voice-default-device = Default
voice-noise-cancel = Noise Cancellation
voice-noise-cancel-desc = Remove background noise from your microphone using AI noise reduction (RNNoise).
voice-noise-cancel-on = Noise Cancellation: On
voice-noise-cancel-off = Noise Cancellation: Off
voice-server-location = Server Location
voice-testing-mic = Testing... (3s)
voice-test-mic = Test Microphone (3 sec)

# Emoji / GIF / Reactions
emoji-picker = Emoji
emoji-search = Search emoji...
emoji-search-results = Search Results
emoji-no-results = No emoji found
gif-picker = GIF
stickers-picker = Stickers
media-picker-gif-placeholder = GIF search coming soon
media-picker-stickers-placeholder = Stickers coming soon
media-picker-markdown = Markdown formatting
reaction-add = Add Reaction

# Message action bar / context menu
msg-reply = Reply
msg-forward = Forward
msg-edit = Edit
msg-delete = Delete
msg-copy-text = Copy Text
msg-apps = Apps
msg-mark-unread = Mark Unread
msg-copy-link = Copy Message Link
msg-speak = Speak Message
msg-report = Report Message
msg-copy-id = Copy Message ID
msg-edit-save = Save
msg-edit-cancel = Cancel

chat-drop-files = Drop files to upload
chat-attach-file = Attach File

# Navigation
nav-back = Back
nav-forward = Forward

# Settings search
settings-search = Search settings...
settings-search-no-results = No settings found matching your search.
settings-search-found = Settings Found
settings-voice-video = Voice & Video
settings-notifications = Notifications
settings-content-social = Content & Social
account-settings-title = Account Settings

# Content & Social settings
content-social-title = Content & Social
content-social-sensitive-media = Sensitive Media
content-social-sensitive-media-desc = Control how sensitive or age-restricted content is displayed in different contexts.
content-social-dm-friends = DMs from friends
content-social-dm-others = DMs from others
content-social-server-channels = Server channels
content-social-show = Show
content-social-hide = Hide
content-social-warn = Warn First
content-social-spam-filter = DM Spam Filter
content-social-spam-filter-desc = Choose how aggressively to filter unsolicited direct messages.
content-social-filter-all = Filter all messages from non-friends
content-social-filter-non-friends = Filter messages from non-friends
content-social-filter-none = Do not filter
content-social-age-restricted = Age-Restricted Content
content-social-age-restricted-servers = Allow access to age-restricted servers
content-social-age-restricted-commands = Allow age-restricted slash commands in DMs
content-social-social-perms = Social Permissions
content-social-social-perms-desc = Control who can contact you and how friend requests work.
content-social-dms-from-members = Allow DMs from server members
content-social-message-requests = Allow message requests from non-friends
content-social-friend-requests = Friend Requests
content-social-fr-everyone = Accept from everyone
content-social-fr-friends-of-friends = Accept from friends of friends
content-social-fr-server-members = Accept from server members
content-social-blocked = Blocked Users
content-social-blocked-desc = Users you have blocked cannot message you or see your profile.
content-social-no-blocked = No blocked users
content-social-unblock = Unblock

# Voice & Video settings
voice-input-device = Input Device
voice-output-device = Output Device
voice-input-volume = Input Volume
voice-output-volume = Output Volume
voice-mic-test = Mic Test
voice-mic-test-stop = Stop Test
voice-input-mode = Input Mode
voice-input-vad = Voice Activity Detection
voice-input-ptt = Push to Talk
voice-noise-suppression = Noise Suppression
voice-noise-off = Off
voice-noise-standard = Standard
voice-noise-high = High
voice-echo-cancel = Echo Cancellation

# Notifications settings
notif-enable-desktop = Enable Desktop Notifications
notif-permission-request = Allow Notifications
notif-global-header = Global (Device)
notif-notify-about = Notify me about
notif-sounds = Sounds
notif-badges = Badges
notif-streams = People I know start streaming
notif-friends-voice = Friends join voice channels
notif-reactions = Someone reacts to my messages
notif-sounds-new-message = New Message
notif-sounds-dm = Direct Messages
notif-sounds-ring = Incoming Ring
notif-badge-unread = Enable Unread Message Badge
notif-no-accounts = No accounts are active. Add an account in Settings → Accounts.

# DM list
dm-saved-messages = Saved Messages
dm-new-conversation = New Conversation
dm-search-conversations = Search Conversations
dm-search-placeholder = Find or start a conversation
saved-items-title = Saved Messages
saved-items-description = Jump back to pinned messages from your DMs and group chats.
saved-items-empty = No pinned messages yet.
saved-items-all-sources = All sources
saved-items-filter-placeholder = Filter saved sources...
saved-items-sources-empty = No saved sources found
dm-no-results = No conversations found

# Friends panel
friends-title = Friends
friends-management-title = People
friends-management-description = Manage friends, ignored users, and blocked users for this account.
friends-management-message = Message
friends-ignored-title = Ignored
friends-ignored-empty = No ignored users yet.
new-conversation-description = Choose one friend to start a direct conversation. Multi-person conversations will use this composer once shared group creation is wired.
new-conversation-start-dm = Start Conversation
new-conversation-group-pending = Multi-person conversations are coming next.
conversation-search-title = Search Conversations
conversation-search-description = Search DMs and group chats for { $account }.
friends-search-placeholder = Search friends...
friends-none = No friends found
friends-demo-empty = This is the demo account — friends appear when you connect real accounts. Click the button below to add one.
friends-demo-add-account = + Add Account
friends-add-friend = + Add Friend
friends-add-coming-soon = Adding friends is coming soon.
notifications-filter-all-types = All notifications
notifications-filter-mentions = Mentions
notifications-filter-friend-requests = Friend requests
notifications-filter-server-invites = Server invites
notifications-filter-voice-invites = Voice invites
notifications-filter-other = Other
notifications-unread-count = unread
filter-all = All Accounts
filter-all-servers = All Servers

# Time-ago formatting
time-just-now = just now
time-one-minute-ago = 1 minute ago
time-minutes-ago = { $count } minutes ago
time-one-hour-ago = 1 hour ago
time-hours-ago = { $count } hours ago
time-one-day-ago = 1 day ago
time-days-ago = { $count } days ago

# Chat extras
chat-toggle-members = Toggle member list
chat-toggle-contact = Toggle contact info
chat-select-channel = Select a channel to start chatting
composer-read-only-notice = This backend is read-only — posts cannot be sent from Poly.

# WP-9 — capability-unsupported placeholders
feature-unsupported-friends = { $backend } doesn't have a friends list.
feature-unsupported-dms = { $backend } doesn't support direct messages.
feature-unsupported-notifications = { $backend } doesn't expose a notification inbox.
feature-unsupported-create-server = { $backend } doesn't support creating servers from Poly.
feature-unsupported-voice = { $backend } doesn't support voice channels.
feature-unsupported-redirecting = Redirecting you back…

# WP-6 — per-plugin container terminology
term-container-server = Server
term-container-server-plural = Servers
term-container-server-create = Create server
term-container-community = Community
term-container-community-plural = Communities
term-container-community-create = Create community
term-container-space = Space
term-container-space-plural = Spaces
term-container-space-create = Create space
term-container-team = Team
term-container-team-plural = Teams
term-container-team-create = Create team
term-container-repo = Repository
term-container-repo-plural = Repositories
term-container-repo-create = Add repository
term-container-feed = Feed
term-container-feed-plural = Feeds
term-container-feed-create = Follow feed
chat-timestamp-yesterday = Yesterday { $time }
search-messages = Search messages
search-placeholder = Search in this channel...
search-placeholder-channel = Search #{ $channel }
search-placeholder-user = Search { $user }
search-placeholder-group = Search { $group }
search-results = Results
search-no-results = No messages matched that search
search-filter-from-user = From a specific user
search-filter-from-user-subtitle = from: user
search-filter-in-channel = Sent in a specific channel
search-filter-in-channel-subtitle = in: channel
search-filter-has-link = Includes a specific type of data
search-filter-has-link-subtitle = has: link, embed or file
search-filter-mentions = Mentions a specific user
search-filter-mentions-subtitle = mentions: user
search-filter-more = More filters
search-filter-more-subtitle = dates, author type and more

# Global Search Page
search-page-title = Search
search-page-placeholder = Search servers, channels, DMs, groups…
search-page-accounts = Accounts
search-page-dms = Direct Messages
search-page-groups = Groups
search-page-type-filter = Show
search-type-servers = Servers
search-type-dms = DMs
search-type-groups = Groups
search-showing-of = Showing { $count } of { $total }
search-load-more = Scroll to load more…

pinned-messages = Pinned messages
no-pinned-messages = No pinned messages
threads = Threads
no-threads = No threads yet
chat-notifications = Notifications
chat-no-notifications = No notifications here
mute-notifications = Mute notifications
unmute-notifications = Unmute notifications
chat-settings = Chat settings
chat-settings-notifications = Notifications
chat-settings-member-list = Member List
chat-settings-grouping = Grouping
chat-settings-grouping-by-status = By status
chat-settings-grouping-none = No grouping
chat-settings-sort-order = Sort order
chat-settings-sort-alphabetical = Alphabetical
chat-settings-sort-online-first = Online first
chat-settings-sort-join-order = Join order
chat-settings-show-offline = Show offline members
user-all-offline-hidden = All members are offline and hidden
filter = Filter
chat-type-message-channel = Message #{ $channel }
chat-type-message-user = Message { $user }
chat-type-message-group = Message { $group }
chat-markdown-formatting = Markdown formatting

# Users extras
account-not-signed-in = Not signed in

# Theme color labels
color-accent = Accent
color-background = Background
color-surface = Surface
color-text = Text
color-secondary-text = Secondary Text
color-border = Border
color-favorites-bar = Favorites Bar Background
color-account-bar = Account Bar Background

# Voice device defaults
voice-default-mic = Default Microphone
voice-default-speakers = Default Speakers

# Error messages
error-storage-unavailable = Storage unavailable
error-load-settings = Failed to load settings
error-reload-servers = Failed to reload servers

# Server context menu
server-menu-mark-read = Mark as Read
server-menu-invite = Invite to Server
server-menu-unmute = Unmute Server
server-menu-mute = Mute Server
server-menu-notif-settings = Notification Settings
server-menu-hide-muted = Hide Muted Channels
server-menu-show-all = Show All Channels
server-menu-privacy = Privacy Settings
server-menu-edit-profile = Edit Per-server Profile
server-menu-leave = Leave Server
server-menu-copy-id = Copy Server ID
server-menu-add-favorites = Add to Favorites
server-menu-remove-favorites = Remove from Favorites

# Channel context menu
channel-menu-mark-read = Mark as Read
channel-menu-mute = Mute Channel
channel-menu-unmute = Unmute Channel
channel-menu-copy-link = Copy Link
channel-menu-copy-id = Copy Channel ID

# DM (1-on-1) context menu
dm-menu-profile = Profile
dm-menu-start-call = Start a Call
dm-menu-add-note = Add Note
dm-menu-add-nickname = Add Friend Nickname
dm-menu-close = Close DM
dm-menu-invite-to-server = Invite to Server
dm-menu-remove-friend = Remove Friend
dm-menu-ignore = Ignore
dm-menu-block = Block
dm-menu-mute = Mute
dm-menu-unmute = Unmute
dm-menu-copy-name = Copy Display Name
dm-menu-copy-user-id = Copy User ID

# Group DM context menu
group-dm-menu-edit = Edit Group
group-dm-menu-invite = Invite Friends to Group DM
group-dm-menu-mute = Mute Conversation
group-dm-menu-unmute = Unmute Conversation
group-dm-menu-leave = Leave Conversation

# Toast labels for context-menu backend ops
dm-action-ok = Done
dm-action-unsupported = Not supported by this backend
dm-action-error = Action failed
dm-action-coming-soon = Coming soon

# Account-icon right-click menu
account-menu-mark-read = Mark Account as Read
account-menu-settings = Account Settings
account-menu-sign-out = Sign Out
account-menu-copy-id = Copy Account ID

# Catch-me-up panel (✨ chat header button)
chat-banner-catch-me-up = Catch me up
catch-up-empty = No recent messages here yet.
catch-up-recent-messages = recent messages
catch-up-copy-prompt = Copy summary prompt
catch-up-copy-prompt-title = Copy a Claude-Desktop-ready summary prompt to your clipboard

# Composer-toolbar typing-simulation button
composer-simulate-typing = Simulate typing
composer-simulate-typing-stop = Stop simulation

# Attachment (image) right-click context menu
attachment-menu-copy-image = Copy Image
attachment-menu-save-image = Save Image
attachment-menu-copy-link = Copy Media Link
attachment-menu-open-link = Open Media Link

# Reaction chip context menu (D2.b)
reaction-menu-show-reactors = Show who reacted
reaction-menu-remove = Remove my reaction

# Remove from favorites inline confirm
remove-favorites-title = Remove "{ $name }" from Favorites?
remove-favorites-body = You can add it back anytime by dragging it to the favorites bar or using this menu.
remove-favorites-cancel = Cancel
remove-favorites-confirm = Remove

# Server banner dropdown menu
server-banner-settings = Server Settings
server-banner-invite = Invite People
server-banner-notif-settings = Notification Settings
server-banner-create-channel = Create Channel
server-banner-channels-roles = Channels & Roles
server-banner-browse-channels = Browse channels and opt into this server's categories.
server-banner-channel-count = channels
server-banner-leave = Leave Server

# Server settings page
server-settings-title = Server Settings
server-settings-overview = Overview
server-settings-notifications = Notifications
server-settings-profile = Profile
server-settings-general = General

# Server overview (icon + banner)
server-overview-icon = Server Icon
server-overview-icon-url = Icon URL
server-overview-icon-hint = URL of the icon image. SVG or PNG with square aspect ratio recommended.
server-overview-banner = Server Banner
server-overview-banner-url = Banner URL
server-overview-banner-hint = URL of the wide banner image shown above the channel list. Landscape format (e.g. 960×240) recommended.
server-overview-save = Save
server-overview-saved = Saved
server-overview-local-override = Override icon locally
server-overview-local-override-hint = This backend doesn't support user-owned server icons. Any icon set here is stored only on this device.

# Leave server inline confirm
leave-server-title = Leave "{ $name }"?
leave-server-body = You won't be able to rejoin unless you are re-invited.
leave-server-cancel = Cancel
leave-server-confirm = Leave Server

# Server notification settings
server-notif-all = All Messages
server-notif-mentions = Only @mentions
server-notif-nothing = Nothing
server-notif-suppress-everyone = Suppress @everyone and @here
server-notif-suppress-roles = Suppress All Role @mentions
server-notif-suppress-highlights = Suppress Highlights
server-notif-mute-events = Mute New Events
server-notif-mobile-push = Mobile Push Notifications

# Server profile settings  
server-profile-nickname = Server Nickname
server-profile-nickname-hint = Change how you appear in this server
server-profile-save = Save Changes

# Server general settings
server-general-info = Server Info
server-general-danger = Danger Zone

# Group DMs
group-members-title = Members
group-member-remove = Remove
group-member-remove-tooltip = Remove { $name } from this group

# DM header
dm-header-subtitle = Direct Message

# Presence status labels
presence-online = Online
presence-away = Away
presence-dnd = Do Not Disturb
presence-offline = Offline

# DM contact panel
dm-contact-panel-title = Contact Info
dm-contact-not-found = Contact not found

# Context menus (shared items)
menu-copy-text = Copy text
menu-copy-id = Copy ID
menu-view-profile = View profile

# Demo backend
demo-regenerate-data = Regenerate Demo Data

# Plugin settings save toast (Pack C.3)
ui-settings-saved = Saved
ui-settings-save-failed = Failed to save setting

# Channel settings page (Pack C.3)
channel-settings-title = Channel Settings
channel-settings-no-plugin-sections = No per-channel settings for this backend.

# Sidebar — stock layout strings (Pack D: P24/P25/P26/P27/P29)
ui-sidebar-nav-label = Sidebar navigation
ui-sidebar-plugin-error = Plugin sidebar failed to load — showing channels

# P24 — SpacesRoomsLayout (Matrix)
ui-sidebar-spaces-header = Spaces
ui-sidebar-spaces-loading = Loading spaces…
ui-sidebar-spaces-error = Failed to load spaces
ui-sidebar-spaces-empty = No spaces joined

# P25 — CommunitiesLayout (Lemmy)
ui-sidebar-communities-header = Communities
ui-sidebar-communities-loading = Loading communities…
ui-sidebar-communities-error = Failed to load communities
ui-sidebar-communities-empty = No communities subscribed
ui-sidebar-communities-tab-subscribed = Subscribed
ui-sidebar-communities-tab-local = Local
ui-sidebar-communities-tab-all = All
ui-sidebar-communities-local-coming-soon = Coming soon — local browse
ui-sidebar-communities-all-coming-soon = Coming soon — federated browse

# P26 — FeedLayout (Hacker News)
ui-sidebar-feed-header = Feeds
ui-sidebar-feed-selected = Selected feed
ui-sidebar-feed-top = Top
ui-sidebar-feed-new = New
ui-sidebar-feed-best = Best
ui-sidebar-feed-ask = Ask
ui-sidebar-feed-show = Show
ui-sidebar-feed-jobs = Jobs

# P27 — RepoTreeLayout (GitHub / Forgejo)
ui-sidebar-repos-header = Repositories
ui-sidebar-repos-loading = Loading repositories…
ui-sidebar-repos-error = Failed to load repositories
ui-sidebar-repos-empty = No repositories connected
ui-sidebar-repo-issues = Issues
ui-sidebar-repo-pulls = Pull Requests
ui-sidebar-repo-discussions = Discussions


# /agent page
nav-agent = Agent
agent-page-title = Agent
agent-search-placeholder = Search agent settings…
agent-section-integrations = Integrations
agent-section-integrations-desc = Hand off Poly's tools to your AI assistant via MCP. No API key needed — Poly runs as an MCP server you add to the Claude app (or any MCP-compatible client).
agent-section-profile = Agent Profile
agent-section-profile-desc = Your shareable handshake card. Other Poly users' agents can ask yours for a quick intro before reaching out — saves the small-talk and gets to the point faster.
agent-profile-textarea-label = Profile
agent-profile-textarea-placeholder = e.g. "Hi, I'm Alex — backend engineer at Aareon, into Rust + WASM, kayaking, and indie horror games. Always up for a chat about plugin architectures."
agent-profile-save = Save profile
agent-profile-visibility-note = Visible to other Poly users your accounts share a chat with. Won't be shared with backends or third parties.
agent-integration-responses = Suggested responses
agent-integration-responses-desc = Let your assistant draft replies you can review before sending.
agent-integration-summaries = Conversation summaries
agent-integration-summaries-desc = Catch up on long threads with on-demand recaps.
agent-integration-translate = Live translation
agent-integration-translate-desc = Translate incoming messages on the fly.
agent-integration-memory = Memory
agent-integration-memory-desc = Per-contact context the assistant carries between conversations.
agent-integration-outreach = Scheduled outreach
agent-integration-outreach-desc = Plan and send "ping every N days" check-ins from your assistant.
agent-integration-image-gen = Image generation
agent-integration-image-gen-desc = Have your assistant generate and attach images on request.

# /agent — per-chat reply style (Phase E)
agent-style-title = Reply style
agent-style-tone = Tone
agent-style-tone-casual = Casual
agent-style-tone-professional = Professional
agent-style-tone-snarky = Snarky
agent-style-tone-warm = Warm
agent-style-tone-direct = Direct
agent-style-formality = Formality
agent-style-formality-tu = Informal (tu / du)
agent-style-formality-vous = Formal (vous / Sie)
agent-style-formality-neutral = Neutral
agent-style-emoji = Emoji allowed
agent-style-signature = Signature
agent-style-extra-notes = Extra notes
agent-style-save = Save

# Empty state shown in ServerHome when the server has no channels yet.
server-empty-title = No channels yet
server-empty-body = This server doesn't have any channels. Ask a moderator to create one, or create the first channel yourself if you have permission.

# Agent panel (🤖 button in chat header)
agent-panel-toggle = Agent panel
agent-panel-title = Agent
agent-panel-access-label = Let agent access this chat
agent-panel-access-description = When on, the connected agent (Claude Desktop or any MCP host) can read context and propose drafts for this chat.
agent-panel-disabled-state = Agent is disabled for this chat
agent-panel-memory-title = Memory
agent-panel-memory-empty = No facts stored yet.
agent-panel-memory-forget = Forget
agent-panel-drafts-title = Pending drafts
agent-panel-drafts-empty = No pending drafts.
agent-panel-style-title = Reply style
agent-panel-activity-title = Recent activity
agent-panel-activity-empty = No agent activity yet.
agent-panel-activity-draft-sent = Sent draft at { $time }
agent-panel-activity-fact-remembered = Remembered fact at { $time }
# Phase B — Draft queue (agent-suggested message drafts)
agent-draft-claude-suggests = ✨ { $suggested_by } suggests:
agent-draft-send = Send
agent-draft-edit = Edit
agent-draft-discard = Discard
agent-draft-autosend-in = Auto-sending in { $secs }s
agent-draft-cancel-autosend = Cancel auto-send
agent-drafts-sidebar-title = Pending drafts
agent-drafts-sidebar-empty = No pending drafts

# Moderation actions — generic
mod-action-kick = Kick member
mod-action-ban = Ban member
mod-action-unban = Unban member
mod-action-timeout = Timeout
mod-action-untimeout = Remove timeout
mod-action-delete-message = Delete message
mod-action-edit-channel = Edit channel

# Backend-native overrides — used when nav.active_backend matches
mod-action-discord-timeout = Timeout
mod-action-discord-ban = Ban
mod-action-matrix-redact = Redact
mod-action-lemmy-ban = Ban from community
mod-action-lemmy-timeout = Temporarily ban

# Settings tabs — moderation
settings-tab-roles = Roles
settings-tab-bans = Bans
settings-tab-modlog = Audit log

# Kick dialog
dialog-kick-title = Kick { $user } from this server?
dialog-kick-reason = Reason (optional)
dialog-kick-confirm = Kick

# Ban dialog
dialog-ban-title = Ban { $user }?
dialog-ban-reason = Reason (optional)
dialog-ban-delete-history = Delete message history
dialog-ban-confirm = Ban

# Timeout dialog
dialog-timeout-title = Timeout { $user }
dialog-timeout-duration = Duration
dialog-timeout-reason = Reason (optional)
dialog-timeout-confirm = Timeout
dialog-timeout-5min = 5 minutes
dialog-timeout-10min = 10 minutes
dialog-timeout-1hr = 1 hour
dialog-timeout-24hr = 24 hours
dialog-timeout-1week = 1 week

# Edit channel dialog
dialog-edit-channel-title = Edit channel
dialog-edit-channel-name = Channel name
dialog-edit-channel-topic = Topic
dialog-edit-channel-slowmode = Slow mode (seconds, 0 = off)
dialog-edit-channel-nsfw = NSFW / Age-gated
dialog-edit-channel-save = Save
dialog-cancel = Cancel

# Moderation action results
dialog-kick-success = Member kicked.
dialog-kick-error = Failed to kick: { $error }
dialog-ban-success = Member banned.
dialog-ban-error = Failed to ban: { $error }
dialog-timeout-success = Timeout applied.
dialog-timeout-error = Failed to apply timeout: { $error }
dialog-edit-channel-success = Channel updated.
dialog-edit-channel-error = Failed to update channel: { $error }

# Bans tab
bans-tab-empty = No bans yet.
bans-tab-unban = Unban
bans-tab-reason-none = (no reason)
bans-tab-unban-success = Unbanned.
bans-tab-unban-error = Failed to unban: { $error }
bans-tab-loading = Loading bans…

# Roles tab
roles-tab-empty = No roles defined.
roles-tab-loading = Loading roles…

# Mod log tab
modlog-tab-empty = No moderation log entries.
modlog-tab-loading = Loading audit log…
modlog-tab-moderator = Moderator
modlog-tab-target = Target
modlog-tab-reason = Reason
modlog-action-kicked = Kicked
modlog-action-banned = Banned
modlog-action-unbanned = Unbanned
modlog-action-timed-out = Timed out
modlog-action-role-updated = Role updated
modlog-action-message-deleted = Message deleted
modlog-action-channel-updated = Channel updated
modlog-action-other = Other: { $detail }

# Per-account overview placeholder (default impl, before each plugin overrides).
overview-default-title = Overview
overview-default-subtitle = This account has no overview defined yet.

account-bar-overview-tooltip = Overview

overview-toggle-servers = Servers
overview-toggle-dms = Direct Messages
overview-toggle-friends = Friends
overview-toggle-notifications = Notifications

# Overview sub-page nav (channel-style sidebar)
overview-page-general = General
overview-page-missed = Things you missed
overview-page-stats = Stats
overview-page-agents = Agents

# Overview sub-page bodies
overview-page-missed-title = Things you missed
overview-page-missed-subtitle = Recent unread notifications and direct messages for this account.
overview-page-stats-title = Stats
overview-page-stats-subtitle = Your activity at a glance.
overview-page-agents-title = Active Agents
overview-page-agents-subtitle = Channels and DMs where you have agent features turned on.
overview-page-agents-empty-title = No agent features active yet
overview-page-agents-empty-body = Open any channel or DM and click the 🤖 agent icon in the header (next to the member list toggle) to activate agent features for that conversation. Active ones will be listed here.
overview-empty-allcaughtup = You're all caught up.
overview-section-unread-dms = Unread Direct Messages
overview-section-unread-notifications = Unread Notifications
overview-stat-servers = Servers
overview-stat-dms = Direct Messages
overview-stat-groups = Groups
overview-stat-unread = Unread
overview-stat-mentions = Mentions

overview-search-placeholder = Search…
