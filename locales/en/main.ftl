# Poly — English (en) main translations
# Project Fluent (.ftl) format

# Application
app-title = Poly
electron-window-minimize = Minimize
electron-window-maximize = Maximize or restore
electron-window-close = Close window
app-description = Multi-platform messenger client

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
setup-welcome-tagline = A multi-account messenger client powered by plugins. Connect all your chat platforms in one place.
setup-feature-plugins = Plugin-based — add support for any messenger via WASM plugins
setup-feature-multi-account = Multi-account — manage all your accounts across platforms
setup-feature-demo = Demo data loaded — explore the app with sample conversations
setup-feature-keys = Identity keys — generate in Settings → Identity when you're ready
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

# Channels
channel-text = Text Channel
channel-voice = Voice Channel
channel-video = Video Channel

# Users
user-online = Online
user-idle = Idle
user-dnd = Do Not Disturb
user-invisible = Invisible
user-offline = Offline
user-members = Members

# Notifications
notifications-title = Notifications
notifications-empty = No new notifications
notifications-mark-read = Mark as Read
notifications-dismiss = Dismiss
notifications-mention = { $user } mentioned you in { $channel }
notifications-friend-request = { $user } sent you a friend request
notifications-server-invite = You've been invited to { $server }

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
settings-backup = Backup Servers
settings-backup-description = Configure encrypted backup sync servers
settings-add-backup = Add Backup Server
settings-identity = Identity
settings-identity-description = Your Poly identity and recovery options
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
settings-language = Language
settings-language-description = Choose your preferred language
settings-appearance = Appearance
settings-appearance-description = Dark mode, light mode, and display options
settings-dark-mode = Dark Mode
settings-light-mode = Light Mode
settings-follow-device = Follow Device Preference
settings-general = General
settings-general-description = Notification preferences and startup behavior
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

# Demo Settings
settings-demo = Demo
settings-demo-description = Manage the built-in demo data client. When enabled, Poly loads sample accounts with servers, channels, and conversations so you can explore the app.
settings-demo-toggle = Enable Demo Data

# Plugin Manager
settings-plugins = Plugins
settings-plugins-description = Enable or disable messenger backend plugins. Each plugin is a messenger client (Demo, Discord, Matrix, Stoat, Teams, Poly Server). Accounts are sessions created by those plugins.
plugins-native-title = Built-in Plugins
plugins-native-description = Toggle messenger backends on or off. Backends marked "not in this build" are not compiled into this version — enabling them here saves your preference for when they become available, or add them as WASM plugins below.
plugins-loaded-count = Active backends
plugins-none-loaded = No WASM plugins added yet. Add a plugin URL below to get started.
plugins-status-disconnected = Disconnected
plugins-status-connecting = Connecting…
plugins-status-connected = Connected
plugins-status-error = Error
plugins-type-native = Native
plugins-type-wasm = WASM
plugins-not-compiled = not in this build
plugins-active-accounts = Active accounts
plugins-wasm-title = WASM Plugins
plugins-wasm-description = WASM plugins extend Poly with additional messenger backends. Load a plugin from a URL — Poly will append its WIT interface version so the server can serve the correct binary.
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
plugins-wit-hint = WIT interface version

# Plugin Settings
settings-plugin-settings = Plugin Settings
# Label shown in the nav sidebar and in the scroll divider before plugin-contributed sections
settings-plugins-section-divider = Plugin-Provided Settings
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
settings-identity-backup-servers = Backup Servers
settings-identity-backup-servers-description = This identity is used for authentication on the following backup servers.
settings-identity-poly-accounts = Poly Server Accounts
settings-identity-poly-accounts-description = This identity is used for the following accounts on self-hosted Poly servers.
settings-identity-no-servers = No backup servers configured yet.
settings-identity-no-poly-accounts = No Poly server accounts.
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
chat-replying-to = Replying to { $name }
action-search = Search
action-copy = Copy
action-back = Back
action-confirm = Confirm

# Errors
error-generic = Something went wrong. Please try again.
error-network = Network error. Check your connection.
error-auth-failed = Authentication failed. Please check your credentials.
error-not-found = Not found.

# Voice / Video
voice-connected = Voice Connected
voice-join-voice = Join Voice
voice-join-video = Join Video
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
voice-go-to-channel = Go to channel
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
gif-picker = GIF
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
settings-voice-video = Voice & Video
settings-notifications = Notifications
account-settings-title = Account Settings

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
dm-search-placeholder = Find or start a conversation
dm-no-results = No conversations found

# Friends panel
friends-title = Friends
friends-search-placeholder = Search friends...
friends-none = No friends found
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
chat-type-message-channel = Message #{ $channel }
chat-type-message-user = Message { $user }
chat-type-message-group = Message { $group }
chat-markdown-formatting = Markdown formatting

# Users extras
user-no-members = No members to show
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

# Demo backend
demo-regenerate-data = Regenerate Demo Data
