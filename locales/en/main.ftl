# Poly — English (en) main translations
# Project Fluent (.ftl) format

# Application
app-title = Poly
app-description = Multi-platform messenger client

# Navigation
nav-dms = Direct Messages
nav-friends = Friends
nav-notifications = Notifications
nav-settings = Settings
nav-servers = Servers
nav-demo = Toggle Demo Client
nav-demo-active = Demo Client Active

# Setup Wizard
setup-welcome-title = Welcome to Poly
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
settings-backup = Backup Servers
settings-backup-description = Configure encrypted backup sync servers
settings-add-backup = Add Backup Server
settings-identity = Identity
settings-identity-description = Your Poly identity and recovery options
settings-your-id = Your Account ID
settings-export-recovery = Export Recovery Phrase
settings-theme = Theme
settings-theme-description = Customize colors, themes, and appearance
settings-theme-preset = Theme Preset
settings-theme-custom-css = Custom CSS
settings-theme-import = Import Theme
settings-theme-export = Export Theme
settings-color-mode = Color Mode
settings-color-overrides = Color Customization
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

# Backup Server Settings
settings-backup-add-server = Add Server
settings-backup-url-placeholder = https://backup.example.com
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
action-search = Search
action-copy = Copy
action-back = Back
action-confirm = Confirm

# Errors
error-generic = Something went wrong. Please try again.
error-network = Network error. Check your connection.
error-auth-failed = Authentication failed. Please check your credentials.
error-not-found = Not found.
