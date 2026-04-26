# Poly — Deutsch (de) Hauptübersetzungen
# Project Fluent (.ftl) Format

# Anwendung
app-title = Poly
electron-window-minimize = Minimieren
electron-window-maximize = Maximieren oder wiederherstellen
electron-window-close = Fenster schließen
app-description = Multi-Plattform-Messenger-Client
wasm-crash-title = Poly ist im Browser abgestürzt
wasm-crash-description = Die aktuelle Seite ist abgestürzt oder hat einen unbehandelten Browser-/WASM-Fehler ausgelöst. Die UI unter diesem Overlay ist nicht mehr vertrauenswürdig.
wasm-crash-details-label = Absturztyp
wasm-crash-location-label = Quellort
wasm-crash-path-label = Route
wasm-crash-reload-action = Poly neu laden
wasm-crash-kind-panic = Rust-Panic
wasm-crash-kind-window-error = Browser-Fehlerereignis
wasm-crash-kind-unhandled-rejection = Unbehandelte Promise-Ablehnung
wasm-crash-kind-unknown = Unbekannter Absturz
wasm-crash-generic-message = Der Browser hat keine Absturzdetails geliefert.
wasm-crash-window-error-fallback = Der Browser hat ein globales Fehlerereignis ohne Meldung gemeldet.
wasm-crash-rejection-fallback = Eine Promise wurde ohne lesbare Fehlermeldung abgelehnt.

# Navigation
nav-dms = Direktnachrichten
nav-friends = Freunde
nav-notifications = Benachrichtigungen
nav-settings = Einstellungen
nav-search = Suche
nav-servers = Server
nav-demo = Demo-Client umschalten
nav-demo-active = Demo-Client aktiv

# Einrichtungsassistent
setup-welcome-title = Willkommen bei Poly
setup-welcome-description = Ein einheitlicher Messenger für alle Ihre Chat-Plattformen.
setup-welcome-tagline = Dein KI-gestütztes soziales Netzwerk. Alle Chats vereint, mit einem KI-Agenten der sich erinnert, antwortet und deine Gespräche verwaltet.
setup-feature-plugins = Plugin-basiert — Discord, Matrix, Teams, Stoat und mehr über WASM-Plugins
setup-feature-multi-account = Einheitlicher Posteingang — alle Konten aller Plattformen an einem Ort
setup-feature-demo = Demo-Daten geladen — erkunde die App mit Beispielunterhaltungen
setup-feature-keys = Identitätsschlüssel — in Einstellungen → Identität generieren, wenn du bereit bist
setup-feature-ai = KI-Agent — Chats zusammenfassen, automatisch antworten und nie ein Gespräch vergessen
setup-feature-translate = Live-Übersetzung — Nachrichten in Echtzeit in jede Sprache übersetzen
setup-get-started = Loslegen
setup-generating-keys = Identitätsschlüssel werden generiert...
setup-your-account-id = Ihre Konto-ID
setup-account-id-description = Dies ist Ihre eindeutige Kennung. Teilen Sie sie mit Freunden, um sich zu verbinden.
setup-recovery-phrase = Wiederherstellungsphrase
setup-recovery-phrase-description = Schreiben Sie diese Wörter auf und bewahren Sie sie sicher auf. Sie benötigen sie zur Wiederherstellung Ihres Kontos.
setup-recovery-warning = Wenn Sie Ihre Wiederherstellungsphrase verlieren, verlieren Sie dauerhaft den Zugang zu Ihrem Konto.
setup-copy-phrase = Phrase kopieren
setup-export-phrase = In Datei exportieren
setup-confirm-phrase = Wiederherstellungsphrase bestätigen
setup-confirm-description = Geben Sie die Wörter Ihrer Wiederherstellungsphrase ein, um zu bestätigen, dass Sie sie gespeichert haben.
setup-continue = Weiter
setup-skip-confirmation = Bestätigung überspringen
setup-complete = Einrichtung abgeschlossen
setup-complete-description = Ihre Identität wurde erstellt. Fügen Sie Messenger-Konten in den Einstellungen hinzu.
setup-go-to-app = Zu Poly

# Chat
chat-type-message = Nachricht eingeben...
chat-send = Senden
chat-typing = { $user } tippt...
chat-typing-multiple = { $count } Personen tippen...
chat-no-messages = Noch keine Nachrichten. Starten Sie die Unterhaltung!
chat-load-more = Mehr laden
chat-edited = (bearbeitet)
chat-loading = Nachrichten werden geladen...
chat-select-conversation = Unterhaltung auswählen
chat-loading-earlier = Ältere Nachrichten werden geladen...
chat-unread-banner = { $count } neue Nachrichten seit { $time} am { $date }
chat-unread-divider = Neu
chat-jump-to-present = Zum Aktuellen springen
chat-viewing-older-messages = Du siehst ältere Nachrichten

# Kanäle
channel-text = Textkanal
channel-voice = Sprachkanal
channel-video = Videokanal

# Benutzer / Status
user-online = Online
user-idle = Abwesend
user-dnd = Nicht stören
user-invisible = Unsichtbar
user-offline = Offline
user-away = Abwesend
user-appear-offline = Als offline anzeigen
user-members = Mitglieder
user-no-members = Keine Mitglieder

# Konto-Bar — Avatar-Ecken-Badges
account-profile-click-hint = Klicken, um dein Profil anzusehen
account-conn-connected = Verbunden
account-conn-connecting = Verbinde…
account-conn-disconnected = Offline
account-conn-error = Verbindungsfehler

# Statuspicker-Popup
status-picker-title = Status setzen

# Mitgliederlistenfilter
member-filter-placeholder = Mitglieder suchen…
member-filter-tooltip = Mitglieder suchen
member-filter-no-results = Keine Mitglieder entsprechen der Suche.

# User profile modal
user-profile-more-options = Weitere Optionen
user-profile-message = Nachricht
user-profile-call = Anruf
user-profile-video = Video
user-profile-add-to-call = Zum Anruf hinzufügen
user-profile-add-video-to-call = Mit Video hinzufügen
user-profile-note = Notiz
user-profile-note-placeholder = Klicken zum Hinzufügen einer Notiz
user-profile-open = Profil anzeigen

# Benachrichtigungen
notifications-title = Benachrichtigungen
notifications-empty = Keine neuen Benachrichtigungen
notifications-mark-read = Als gelesen markieren
notifications-dismiss = Schließen
notifications-mention = { $user } hat Sie in { $channel } erwähnt
notifications-friend-request = { $user } hat Ihnen eine Freundschaftsanfrage gesendet
notifications-server-invite = Sie wurden zu { $server } eingeladen

# Einstellungen
settings-title = Einstellungen
settings-accounts = Konten
settings-accounts-description = Verwalten Sie Ihre Messenger-Konten
settings-add-account = Konto hinzufügen
settings-remove-account = Konto entfernen
settings-no-accounts = Keine Konten verbunden. Füge ein Konto hinzu, um loszulegen.
settings-account-settings-link = Kontoeinstellungen
account-switch = Konto wechseln
account-settings = Kontoeinstellungen
settings-account-settings = Kontoeinstellungen

# Anmelde-Flow — Backend-Auswahl
signup-picker-title = Konto hinzufügen
signup-picker-description = Wählen Sie, welche Art von Konto hinzugefügt werden soll.
signup-picker-back = ← Zurück zu Einstellungen
signup-stub-back = ← Backend wählen
# ── Server / Kanal erstellen ─────────────────────────────────────────────────
create-server-btn = Server erstellen
create-server-placeholder = Servername…
create-server-submit = Erstellen
create-server-cancel = Abbrechen
create-server-creating = Erstelle…
create-server-page-title = Server erstellen
create-server-page-subtitle = Gib deinem Server einen Namen. Du kannst ihn später jederzeit ändern.
create-server-page-label = Servername
channel-list-text-channels = Textkanäle
create-channel-btn = Neuer Kanal
create-channel-page-title = Kanal erstellen
create-channel-page-subtitle = Gib deinem Kanal einen Namen. Du kannst ihn später jederzeit ändern.
create-channel-page-label = Kanalname
create-channel-placeholder = Kanalname…
create-channel-submit = Erstellen
create-channel-cancel = Abbrechen
create-channel-creating = Erstelle…
settings-backup = Backup-Server
settings-backup-description = Verschlüsselte Backup-Sync-Server konfigurieren
settings-add-backup = Backup-Server hinzufügen
settings-identity = Identität
settings-identity-description = Deine Geräte-Identität, Wiederherstellungsphrase und wo diese Identität verwendet wird
settings-your-id = Ihre Konto-ID
settings-export-recovery = Wiederherstellungsphrase exportieren
settings-theme = Design
settings-theme-description = Farben, Designs und Erscheinungsbild anpassen
settings-media = Medien
settings-media-description = GIF-Anbieter und zukünftige Rich-Media-Integrationen konfigurieren
settings-media-active-provider = Aktiver GIF-Anbieter
settings-media-api-key = API-Schlüssel
settings-media-api-key-placeholder = API-Schlüssel des Anbieters einfügen
settings-media-provider-klippy = Klippy
settings-media-provider-giphy = Giphy
settings-media-provider-imgur = Imgur
settings-media-status-configured = Konfiguriert
settings-media-status-not-setup = Nicht eingerichtet
settings-theme-preset = Design-Vorlage
settings-theme-custom-css = Benutzerdefiniertes CSS
settings-theme-import = Design importieren
settings-theme-export = Design exportieren
settings-color-mode = Farbmodus
settings-color-overrides = Farbanpassung
settings-color-hint = Aktivieren Sie diese Option, um einzelne Farben von der Vorlage zu überschreiben. Deaktivieren Sie, um zur Vorlage zurückzukehren.
settings-reset-colors = Farben zurücksetzen
settings-theme-apply-css = CSS anwenden
settings-css-hint = Kommentierung einer Variable aufheben, um die Design-Vorlage zu überschreiben. Der Schalter aktiviert/deaktiviert diese CSS-Anpassungen.
settings-css-reset-template = Vorlage zurücksetzen
settings-language = Sprache
settings-language-description = Wählen Sie Ihre bevorzugte Sprache
settings-appearance = Erscheinungsbild
settings-appearance-description = Dunkel-/Hellmodus und Anzeigeoptionen
settings-dark-mode = Dunkelmodus
settings-light-mode = Hellmodus
settings-follow-device = Geräteeinstellung folgen
settings-layout = Layout
settings-layout-description = Layout-Verhalten und Spiegelung über Desktop- und Mobil-Shell hinweg
settings-general = Allgemein
settings-general-description = Lokale App-Daten zurücksetzen oder den Zustand vollständig für sauberes Re-Testing nuken
settings-layout-mode = Layout-Modus
settings-layout-mode-description = Lege fest, ob Poly Mobil per Breite, per Hochformat oder immer als Desktop/Mobil erzwingen soll. URL-Overrides wie ?layout=mobile oder ?layout=desktop haben Vorrang, solange sie gesetzt sind.
settings-layout-auto-width = Auto (Breite ≤ 640px)
settings-layout-auto-portrait = Auto (Hochformat)
settings-layout-force-desktop = Desktop erzwingen
settings-layout-force-mobile = Mobil erzwingen
settings-mirror-menu-layout = App-Menüs / Wings spiegeln
settings-mirror-menu-layout-description = Tauscht die linken und rechten App-Wings auf Desktop und Mobil, einschließlich Sidebar-Reihenfolge und mobiler Header-Buttons.
settings-mirror-chat-messages = Chat-Nachrichten spiegeln
settings-mirror-chat-messages-description = Platziert Avatare / Nachrichtengutter rechts, während der Text normal lesbar bleibt.
settings-force-mobile-layout = Mobiles Layout erzwingen
settings-force-mobile-layout-description = Verwende die mobile Shell auch oberhalb von 640px. Deaktiviere dies, um das Desktop-Layout zu behalten, bis das Fenster von selbst schmal genug ist.
settings-reset-description = Setze App-Daten für einen Neustart zurück oder zerstöre den gesamten lokalen Zustand für sauberes Re-Testing.
settings-reset-app = App-Daten zurücksetzen
settings-nuke-app = App-Zustand NUKEN
settings-reset-error-no-storage = Speicher ist noch nicht bereit
settings-reset-error-failed = App-Daten konnten nicht zurückgesetzt werden
settings-nuke-error-failed = App-Zustand konnte nicht genuket werden
settings-reset-error-reload = Zurücksetzen erfolgreich, aber Neuladen fehlgeschlagen

# Demo-Einstellungen
settings-demo = Demo
settings-demo-description = Verwalte den integrierten Demo-Datenclient. Wenn aktiviert, lädt Poly Beispielkonten mit Servern, Kanälen und Unterhaltungen, damit du die App erkunden kannst.
settings-demo-toggle = Demo-Daten aktivieren

# Plugin-Manager
settings-plugins = Plugins
settings-plugins-description = Jedes Messenger-Backend in Poly ist ein WASM-Plugin. Integrierte Plugins werden mit der App ausgeliefert; nachgeladene Plugins fügst du zur Laufzeit hinzu. Konten sind von Plugins erzeugte Sitzungen.
plugins-builtin-title = Integrierte WASM-Plugins
plugins-builtin-description = Mit Poly mitgelieferte Plugins. Werden zusammen mit der App aktualisiert. Backends mit „nicht in diesem Build" sind in dieser Version nicht einkompiliert — aktiviere sie hier, um die Auswahl zu speichern, oder füge sie unten als nachgeladenes Plugin hinzu.
plugins-loaded-count = Aktive Backends
plugins-none-loaded = Noch keine nachgeladenen Plugins. Füge unten eine URL ein oder lade eine .wasm-Datei hoch.
plugins-status-disconnected = Getrennt
plugins-status-connecting = Verbinde…
plugins-status-connected = Verbunden
plugins-status-error = Fehler
plugins-type-builtin = Integriert
plugins-type-sideloaded = Nachgeladen
plugins-type-bundled = Mitgeliefert
plugins-not-compiled = nicht in diesem Build
plugins-active-accounts = Aktive Konten
plugins-sideloaded-title = Nachgeladene WASM-Plugins
plugins-sideloaded-description = Vom Benutzer installierte Plugins. Füge sie unten hinzu — als URL oder lokale .wasm-Datei. Nachgeladene Plugins werden nicht automatisch mit Poly aktualisiert; zum Aktualisieren erneut hinzufügen.
plugins-add-wasm-title = Plugin von URL hinzufügen
plugins-add-wasm-description = Gib die Basis-URL eines WASM-Plugins ein. Die WIT-Version wird automatisch angehängt.
plugins-url-placeholder = https://plugins.example.com/matrix.wasm
plugins-name-placeholder = Anzeigename (optional)
plugins-add-btn = Plugin hinzufügen
plugins-url-required = Bitte eine Plugin-URL eingeben
plugins-remove = Entfernen
plugins-remove-confirm = Dieses Plugin entfernen?
plugins-remove-yes = Ja, entfernen
plugins-remove-cancel = Abbrechen
plugins-wit-hint = WIT-Schnittstellenversion

# Plugin-Einstellungen
settings-plugin-settings = Plugin-Einstellungen
# Beschriftung vor plugin-eigenen Abschnitten in der Einstellungs-Sidebar
settings-plugins-section-divider = Plugin-Einstellungen
# Gruppen-Header in der Einstellungs-Sidebar vor Plugin-Seiten
settings-plugin-settings-nav-header = Plugin-Einstellungen
# Kleines Badge für plugin-eigene Abschnitte
settings-plugins-badge = Plugin
plugin-settings-nav-title = Backend-Einstellungen
plugin-settings-none = Keine Backends mit Einstellungen geladen. Aktiviere Demo-Daten oder verbinde ein Konto.
plugin-settings-generic-description = Dieses Backend hat noch keine benutzerdefinierten Einstellungen. Einstellungen erscheinen hier, wenn das Plugin sie unterstützt.
# Hinweis: plugin-demo-* Strings werden aus dem FTL-Bundle des Demo-Plugins geladen.

# Backup Server Settings
settings-backup-add-server = Server hinzufügen
settings-backup-url-placeholder = http://127.0.0.1:8080
settings-backup-url-label = Server-URL
settings-backup-label-label = Servername
settings-backup-passphrase-label = Server-Passphrase
settings-backup-connect = Verbinden
settings-backup-connecting = Verbinde...
settings-backup-cancel = Abbrechen
settings-backup-status-unknown = Unbekannt
settings-backup-status-connected = Verbunden
settings-backup-status-auth-required = Authentifizierung erforderlich
settings-backup-status-unreachable = Nicht erreichbar
settings-backup-status-syncing = Synchronisiere...
settings-backup-sync-now = Jetzt synchronisieren
settings-backup-reauth = Neu authentifizieren
settings-backup-remove = Entfernen
settings-backup-last-synced = Zuletzt synchronisiert: { $time }
settings-backup-never-synced = Noch nie synchronisiert
settings-backup-enabled = Aktiviert
settings-backup-auth-success = Verbunden!
settings-backup-auth-failed = Authentifizierung fehlgeschlagen
settings-backup-no-servers = Keine Backup-Server konfiguriert.
settings-backup-wizard-step1 = Server-URL
settings-backup-wizard-step2 = Verbinden
settings-backup-step1-hint = Gib die URL deines Poly-Backup-Servers ein
settings-backup-step2-hint = Vergib einen Namen und gib die Zugangsdaten ein
settings-backup-check-btn = Verbindung prüfen
settings-backup-checking = Wird geprüft…
settings-backup-continue = Weiter
settings-backup-back = Zurück
settings-backup-finish = Einrichtung abschließen
settings-backup-url-empty = Bitte gib eine Server-URL ein
settings-backup-password-required = 🔒 Passwort erforderlich
settings-backup-no-password-required = ✓ Kein Passwort erforderlich
settings-backup-server-full = Server ist voll — Registrierungen deaktiviert

# Identity Settings
settings-identity-your-id-label = Deine Poly-Konto-ID
settings-identity-copy-id = ID kopieren
settings-identity-show-phrase = Wiederherstellungsphrase anzeigen
settings-identity-phrase-modal-title = Deine Wiederherstellungsphrase
settings-identity-phrase-warning = Halte diese Phrase geheim. Jeder, der sie hat, kann auf dein Konto zugreifen.
settings-identity-copy-all = Alle Wörter kopieren
settings-identity-close = Schließen
settings-identity-no-identity = Identität noch nicht generiert. Schließe zuerst den Einrichtungsassistenten ab.
settings-identity-create-btn = Identität erstellen
settings-identity-creating = Wird erstellt…
settings-identity-purpose = Dieses Identitätsmaterial wird von Poly in deinem Namen verwendet:
settings-identity-purpose-poly = Poly-Server verwenden es für schlüsselbasierte Anmeldung und Ende-zu-Ende-verschlüsselte Funktionen.
settings-identity-purpose-backup = Backup-Server verwenden es zur Ableitung von Verschlüsselungsschlüsseln und zur Authentifizierung verschlüsselter Synchronisierung.
settings-identity-backup-servers = Backup-Server
settings-identity-backup-servers-description = Diese Identität wird zur Authentifizierung auf den folgenden Backup-Servern verwendet.
settings-identity-poly-accounts = Poly-Server-Konten
settings-identity-poly-accounts-description = Diese Identität wird für die folgenden Konten auf selbstgehosteten Poly-Servern verwendet.
settings-identity-no-servers = Noch keine Backup-Server konfiguriert.
settings-identity-no-poly-accounts = Keine Poly-Server-Konten.
settings-identity-status-active = Aktiv
settings-identity-status-disabled = Deaktiviert
settings-identity-delete = Identität löschen
settings-identity-delete-confirm-title = Identität löschen?
settings-identity-delete-confirm-message = Dies wird diesen Identitätsschlüssel dauerhaft entfernen. Stelle sicher, dass du die Wiederherstellungsphrase gesichert hast, sonst kannst du den Zugriff nicht wiederherstellen!
settings-identity-delete-confirm = Ja, löschen
settings-identity-cancel = Abbrechen

# Design-Vorlagen
theme-blue = Blau
theme-purple = Lila
theme-red = Rot
theme-green = Grün
theme-monotone = Monoton

# Backends
backend-stoat = Stoat
backend-matrix = Matrix
backend-discord = Discord
backend-teams = Teams
backend-demo = Demo

# Allgemeine Aktionen
action-save = Speichern
action-cancel = Abbrechen
action-delete = Löschen
action-edit = Bearbeiten
action-close = Schließen
action-more = Mehr
chat-replying-to = Antwort an { $name }
action-search = Suchen
action-copy = Kopieren
action-back = Zurück
action-confirm = Bestätigen
action-clear = Leeren
action-download = Herunterladen
action-open-in-browser = Im Browser öffnen
zoom-in = Vergrößern
zoom-out = Verkleinern

media-viewer-unavailable-title = Medium nicht verfügbar
media-viewer-unavailable-body = Dieses Medium konnte aus dem aktuellen Chat-Zustand nicht geladen werden.

# Fehler
error-generic = Etwas ist schiefgelaufen. Bitte versuchen Sie es erneut.
error-network = Netzwerkfehler. Überprüfen Sie Ihre Verbindung.
error-auth-failed = Authentifizierung fehlgeschlagen. Bitte überprüfen Sie Ihre Anmeldedaten.
error-not-found = Nicht gefunden.

# Sprache / Video
voice-connected = Sprache verbunden
voice-join-voice = Sprache beitreten
voice-join-video = Video beitreten
voice-direct-call = Direktanruf
voice-group-call = Gruppenanruf
voice-swap-held-call = Gehaltenen Anruf wechseln
voice-disconnect = Trennen
voice-muted = Stummgeschaltet
voice-deafened = Taub geschaltet
voice-streaming = Bildschirm teilen
voice-video-on = Kamera an
voice-mute = Stummschalten
voice-unmute = Stummschaltung aufheben
voice-deafen = Taub schalten
voice-undeafen = Taub aufheben
voice-no-channel = Kein Kanal ausgewählt
voice-no-one-here = Noch niemand hier
voice-be-first = Sei der Erste, der beitritt!
voice-watching-screen = Bildschirmfreigabe ansehen
voice-in-channel = im Kanal
voice-in-call = im Anruf
voice-go-to-channel = Zum Kanal
voice-go-to-conversation = Zur Unterhaltung
direct-call-calling = Rufe an…
direct-call-calling-video = Starte Videoanruf…
direct-call-adding = Füge zum Anruf hinzu…
direct-call-adding-video = Füge mit Video zum Anruf hinzu…
direct-call-awaiting-join = Warte auf Verbindungsaufbau
direct-call-ringing = Klingelt… tippe auf × zum Abbrechen
direct-call-cancel = Anruf abbrechen
voice-mute-mic = Mikrofon stummschalten
voice-unmute-mic = Mikrofon aktivieren
voice-camera = Kamera ein/aus
voice-screen-share = Bildschirm teilen
mobile-nav-open = Navigationsmenü öffnen
mobile-nav-close = Navigationsmenü schließen
voice-activity = Aktivität teilen
voice-voiceboard = Sprachboard
voice-signal-quality = Signalqualität
voice-stop-camera = Kamera beenden
voice-stop-share = Teilen beenden
voice-camera-preview = Kamera-Vorschau
voice-screen-sharing = Bildschirmfreigabe-Vorschau
voice-audio-settings = Sprach- & Audioeinstellungen
voice-mic-device = Eingabegerät (Mikrofon)
voice-speaker-device = Ausgabegerät (Lautsprecher)
voice-default-device = Standard
voice-noise-cancel = Geräuschunterdrückung
voice-noise-cancel-desc = Hintergrundgeräusche per KI-Rauschunterdrückung (RNNoise) entfernen.
voice-noise-cancel-on = Geräuschunterdrückung: An
voice-noise-cancel-off = Geräuschunterdrückung: Aus
voice-server-location = Serverstandort
voice-testing-mic = Testen... (3s)
voice-test-mic = Mikrofon testen (3 Sek.)

# Emoji / GIF / Reaktionen
emoji-picker = Emoji
emoji-search = Emoji suchen...
gif-picker = GIF
stickers-picker = Sticker
media-picker-gif-placeholder = GIF-Suche kommt bald
media-picker-stickers-placeholder = Sticker kommen bald
media-picker-markdown = Markdown-Formatierung
reaction-add = Reaktion hinzufügen

# Nachrichten-Aktionsleiste / Kontextmenü
msg-reply = Antworten
msg-forward = Weiterleiten
msg-edit = Bearbeiten
msg-delete = Löschen
msg-copy-text = Text kopieren
msg-apps = Apps
msg-mark-unread = Als ungelesen markieren
msg-copy-link = Nachrichtenlink kopieren
msg-speak = Nachricht vorlesen
msg-report = Nachricht melden
msg-copy-id = Nachrichten-ID kopieren
msg-edit-save = Speichern
msg-edit-cancel = Abbrechen

chat-drop-files = Dateien zum Hochladen ablegen
chat-attach-file = Datei anhängen

# Navigation
nav-back = Zurück
nav-forward = Vorwärts

# Settings search
settings-search = Einstellungen durchsuchen...
settings-search-no-results = Keine Einstellungen für diese Suche gefunden.
settings-search-found = Einstellungen Gefunden
settings-voice-video = Sprache & Video
settings-notifications = Benachrichtigungen
account-settings-title = Kontoeinstellungen

# Voice & Video settings
voice-input-device = Eingabegerät
voice-output-device = Ausgabegerät
voice-input-volume = Eingabelautstärke
voice-output-volume = Ausgabelautstärke
voice-mic-test = Mikrofon testen
voice-mic-test-stop = Test beenden
voice-input-mode = Eingabemodus
voice-input-vad = Sprachaktivitätserkennung
voice-input-ptt = Sprechtaste
voice-noise-suppression = Rauschunterdrückung
voice-noise-off = Aus
voice-noise-standard = Standard
voice-noise-high = Hoch
voice-echo-cancel = Echounterdrückung

# Notifications settings
notif-enable-desktop = Desktop-Benachrichtigungen aktivieren
notif-permission-request = Benachrichtigungen erlauben
notif-global-header = Global (Gerät)
notif-notify-about = Benachrichtige mich über
notif-sounds = Töne
notif-badges = Abzeichen
notif-streams = Bekannte starten einen Stream
notif-friends-voice = Freunde treten Sprachkanälen bei
notif-reactions = Jemand reagiert auf meine Nachrichten
notif-sounds-new-message = Neue Nachricht
notif-sounds-dm = Direktnachrichten
notif-sounds-ring = Eingehender Anruf
notif-badge-unread = Ungelesene-Nachrichten-Badge aktivieren
notif-no-accounts = Keine Konten aktiv. Konto unter Einstellungen → Konten hinzufügen.

# DM list
dm-saved-messages = Gespeicherte Nachrichten
dm-new-conversation = Neue Unterhaltung
dm-search-conversations = Unterhaltungen suchen
dm-search-placeholder = Gespräch suchen oder starten
saved-items-title = Gespeicherte Nachrichten
saved-items-description = Springe zurück zu angehefteten Nachrichten aus deinen Direktnachrichten und Gruppenchats.
saved-items-empty = Noch keine angehefteten Nachrichten.
saved-items-all-sources = Alle Quellen
saved-items-filter-placeholder = Gespeicherte Quellen filtern...
saved-items-sources-empty = Keine gespeicherten Quellen gefunden
dm-no-results = Kein Gespräch gefunden

# Friends panel
friends-title = Freunde
friends-management-title = Personen
friends-management-description = Verwalte Freunde, ignorierte Nutzer und blockierte Nutzer für dieses Konto.
friends-management-message = Nachricht senden
friends-ignored-title = Ignoriert
friends-ignored-empty = Noch keine ignorierten Nutzer.
new-conversation-description = Wähle einen Freund aus, um eine Direktunterhaltung zu starten. Mehrpersonen-Unterhaltungen nutzen diesen Composer, sobald die gemeinsame Gruppenerstellung angebunden ist.
new-conversation-start-dm = Unterhaltung starten
new-conversation-group-pending = Unterhaltungen mit mehreren Personen kommen als Nächstes.
conversation-search-title = Unterhaltungen suchen
conversation-search-description = Durchsuche Direktnachrichten und Gruppenchats für { $account }.
friends-search-placeholder = Freunde durchsuchen...
friends-none = Keine Freunde gefunden
friends-demo-empty = Dies ist das Demo-Konto — Freunde erscheinen, wenn du echte Konten verbindest. Klicke unten, um eines hinzuzufügen.
friends-demo-add-account = + Konto hinzufügen
friends-add-friend = + Freund hinzufügen
friends-add-coming-soon = Freunde hinzufügen kommt bald.
notifications-filter-all-types = Alle Benachrichtigungen
notifications-filter-mentions = Erwähnungen
notifications-filter-friend-requests = Freundschaftsanfragen
notifications-filter-server-invites = Servereinladungen
notifications-filter-voice-invites = Spracheinladungen
notifications-filter-other = Andere
notifications-unread-count = ungelesen
filter-all = Alle Konten
filter-all-servers = Alle Server

# Zeitformatierung
time-just-now = gerade eben
time-one-minute-ago = vor 1 Minute
time-minutes-ago = vor { $count } Minuten
time-one-hour-ago = vor 1 Stunde
time-hours-ago = vor { $count } Stunden
time-one-day-ago = vor 1 Tag
time-days-ago = vor { $count } Tagen

# Chat-Extras
chat-toggle-members = Mitgliederliste ein-/ausblenden
chat-toggle-contact = Kontaktinfo ein-/ausblenden
chat-select-channel = Wähle einen Kanal, um zu chatten
chat-timestamp-yesterday = Gestern { $time }
search-messages = Nachrichten durchsuchen
search-placeholder = In diesem Kanal suchen...
search-placeholder-channel = #{ $channel } durchsuchen
search-placeholder-user = { $user } durchsuchen
search-placeholder-group = { $group } durchsuchen
search-results = Ergebnisse
search-no-results = Keine Nachrichten passen zu dieser Suche
search-filter-from-user = Von einer bestimmten Person
search-filter-from-user-subtitle = von: Person
search-filter-in-channel = In einem bestimmten Kanal gesendet
search-filter-in-channel-subtitle = in: Kanal
search-filter-has-link = Enthält einen bestimmten Datentyp
search-filter-has-link-subtitle = hat: Link, Einbettung oder Datei
search-filter-mentions = Erwähnt eine bestimmte Person
search-filter-mentions-subtitle = erwähnt: Person
search-filter-more = Weitere Filter
search-filter-more-subtitle = Daten, Autorentyp und mehr
pinned-messages = Gepinnte Nachrichten
no-pinned-messages = Keine gepinnten Nachrichten
threads = Threads
no-threads = Noch keine Threads
chat-notifications = Benachrichtigungen
chat-no-notifications = Hier gibt es keine Benachrichtigungen
chat-type-message-channel = Nachricht in #{ $channel }
chat-type-message-user = Nachricht an { $user }
chat-type-message-group = Nachricht an { $group }
chat-markdown-formatting = Markdown-Formatierung

# Benutzer-Extras
user-all-offline-hidden = Alle Mitglieder sind offline und werden ausgeblendet
account-not-signed-in = Nicht angemeldet

# Chat-Einstellungen — Mitgliederliste
chat-settings-member-list = Mitgliederliste
chat-settings-grouping = Gruppierung
chat-settings-grouping-by-status = Nach Status
chat-settings-grouping-none = Keine Gruppierung
chat-settings-sort-order = Sortierreihenfolge
chat-settings-sort-alphabetical = Alphabetisch
chat-settings-sort-online-first = Online zuerst
chat-settings-sort-join-order = Beitrittsreihenfolge
chat-settings-show-offline = Offline-Mitglieder anzeigen

# Farbbezeichnungen
color-accent = Akzent
color-background = Hintergrund
color-surface = Oberfläche
color-text = Text
color-secondary-text = Sekundärer Text
color-border = Rahmen
color-favorites-bar = Hintergrund Favoritenleiste
color-account-bar = Hintergrund Kontobereich

# Audiogeräte-Standards
voice-default-mic = Standard-Mikrofon
voice-default-speakers = Standard-Lautsprecher

# Fehlermeldungen
error-storage-unavailable = Speicher nicht verfügbar
error-load-settings = Einstellungen konnten nicht geladen werden
error-reload-servers = Server konnten nicht neu geladen werden

# Server context menu
server-menu-mark-read = Als gelesen markieren
server-menu-invite = Zum Server einladen
server-menu-unmute = Server stummschalten aufheben
server-menu-mute = Server stummschalten
server-menu-notif-settings = Benachrichtigungseinstellungen
server-menu-hide-muted = Stummgeschaltete Kanäle ausblenden
server-menu-show-all = Alle Kanäle anzeigen
server-menu-privacy = Datenschutzeinstellungen
server-menu-edit-profile = Serverprofil bearbeiten
server-menu-leave = Server verlassen
server-menu-copy-id = Server-ID kopieren
server-menu-add-favorites = Zu Favoriten hinzufügen
server-menu-remove-favorites = Aus Favoriten entfernen

# Attachment (image) right-click context menu
attachment-menu-copy-image = Bild kopieren
attachment-menu-save-image = Bild speichern
attachment-menu-copy-link = Medien-Link kopieren
attachment-menu-open-link = Medien-Link öffnen

# Reaction chip context menu (D2.b)
reaction-menu-show-reactors = Wer reagiert hat anzeigen
reaction-menu-remove = Meine Reaktion entfernen

# Remove from favorites inline confirm
remove-favorites-title = "{ $name }" aus Favoriten entfernen?
remove-favorites-body = Sie können es jederzeit erneut hinzufügen, indem Sie es in die Favoritenleiste ziehen oder diese Menü verwenden.
remove-favorites-cancel = Abbrechen
remove-favorites-confirm = Entfernen

# Server-Banner-Dropdown-Menü
server-banner-settings = Server-Einstellungen
server-banner-invite = Personen einladen
server-banner-notif-settings = Benachrichtigungseinstellungen
server-banner-create-channel = Kanal erstellen
server-banner-channels-roles = Kanäle & Rollen
server-banner-browse-channels = Kanäle durchsuchen und in Kategorien dieses Servers einsteigen.
server-banner-channel-count = Kanäle
server-banner-leave = Server verlassen

# Server settings page
# Server-Einstellungen
server-settings-title = Server-Einstellungen
server-settings-overview = Übersicht
server-settings-notifications = Benachrichtigungen
server-settings-profile = Profil
server-settings-general = Allgemein

# Server-Übersicht (Icon + Banner)
server-overview-icon = Server-Icon
server-overview-icon-url = Icon-URL
server-overview-icon-hint = URL des Icon-Bildes. SVG oder PNG im quadratischen Format empfohlen.
server-overview-banner = Server-Banner
server-overview-banner-url = Banner-URL
server-overview-banner-hint = URL des breiten Bannerbilds oberhalb der Kanalliste. Querformat (z. B. 960×240) empfohlen.
server-overview-save = Speichern
server-overview-saved = Gespeichert
server-overview-local-override = Icon lokal überschreiben
server-overview-local-override-hint = Dieses Backend unterstützt keine benutzereigenen Server-Icons. Das hier festgelegte Icon wird nur auf diesem Gerät gespeichert.

# Leave server inline confirm
leave-server-title = "{ $name }" verlassen?
leave-server-body = Du kannst nur wieder beitreten, wenn du erneut eingeladen wirst.
leave-server-cancel = Abbrechen
leave-server-confirm = Server verlassen

# Server notification settings
server-notif-all = Alle Nachrichten
server-notif-mentions = Nur @Erwähnungen
server-notif-nothing = Nichts
server-notif-suppress-everyone = @everyone und @here unterdrücken
server-notif-suppress-roles = Alle Rollen-@Erwähnungen unterdrücken
server-notif-suppress-highlights = Highlights unterdrücken
server-notif-mute-events = Neue Ereignisse stummschalten
server-notif-mobile-push = Mobile Push-Benachrichtigungen

# Server profile settings
server-profile-nickname = Server-Nickname
server-profile-nickname-hint = Ändere, wie du auf diesem Server erscheinst
server-profile-save = Änderungen speichern

# Server general settings
server-general-info = Serverinformationen
server-general-danger = Gefahrenzone

# Group DMs
group-members-title = Mitglieder
group-member-remove = Entfernen
group-member-remove-tooltip = { $name } aus dieser Gruppe entfernen

# DM header
dm-header-subtitle = Direktnachricht

# Presence status labels
presence-online = Online
presence-away = Abwesend
presence-dnd = Nicht stören
presence-offline = Offline

# DM contact panel
dm-contact-panel-title = Kontaktinfo
dm-contact-not-found = Kontakt nicht gefunden

# Demo backend
demo-regenerate-data = Demodaten neu generieren

# Search page
search-page-title = Suche
search-page-placeholder = Server, Kanäle, DMs, Gruppen suchen…
search-page-accounts = Konten
search-page-dms = Direktnachrichten
search-page-groups = Gruppen
search-page-type-filter = Anzeigen
search-type-servers = Server
search-type-dms = DMs
search-type-groups = Gruppen
search-showing-of = { $count } von { $total } angezeigt
search-load-more = Scrollen für mehr…

# Context menus (shared items)
menu-copy-text = Copy text
menu-copy-id = Copy ID
menu-view-profile = View profile

# Plugin settings save toast (Pack C.3)
ui-settings-saved = Gespeichert
ui-settings-save-failed = Einstellung konnte nicht gespeichert werden

# Channel settings page (Pack C.3)
channel-settings-title = Kanal-Einstellungen
channel-settings-no-plugin-sections = Keine kanalspezifischen Einstellungen für dieses Backend.

# Sidebar-Standardlayout-Zeichenfolgen (P24/P25/P26/P27/P29)
# Mirrors keys added in locales/en/main.ftl at the same offset.
ui-sidebar-nav-label = Sidebar-Navigation
ui-sidebar-plugin-error = Plugin-Sidebar konnte nicht geladen werden — Kanäle werden angezeigt
ui-sidebar-spaces-header = Spaces
ui-sidebar-spaces-loading = Spaces werden geladen…
ui-sidebar-spaces-error = Spaces konnten nicht geladen werden
ui-sidebar-spaces-empty = Keine Spaces beigetreten
ui-sidebar-communities-header = Communities
ui-sidebar-communities-loading = Communities werden geladen…
ui-sidebar-communities-error = Communities konnten nicht geladen werden
ui-sidebar-communities-empty = Keine Communities abonniert
ui-sidebar-communities-tab-subscribed = Abonniert
ui-sidebar-communities-tab-local = Lokal
ui-sidebar-communities-tab-all = Alle
ui-sidebar-communities-local-coming-soon = Demnächst verfügbar — lokales Durchsuchen
ui-sidebar-communities-all-coming-soon = Demnächst verfügbar — föderiertes Durchsuchen
ui-sidebar-feed-header = Feeds
ui-sidebar-feed-selected = Ausgewählter Feed
ui-sidebar-feed-top = Top
ui-sidebar-feed-new = Neu
ui-sidebar-feed-best = Beste
ui-sidebar-feed-ask = Ask
ui-sidebar-feed-show = Show
ui-sidebar-feed-jobs = Jobs
ui-sidebar-repos-header = Repositories
ui-sidebar-repos-loading = Repositories werden geladen…
ui-sidebar-repos-error = Repositories konnten nicht geladen werden
ui-sidebar-repos-empty = Keine Repositories verbunden
ui-sidebar-repo-issues = Issues
ui-sidebar-repo-pulls = Pull Requests
ui-sidebar-repo-discussions = Diskussionen


# /agent page
nav-agent = Agent
agent-page-title = Agent
agent-search-placeholder = Agenteneinstellungen durchsuchen…
agent-section-integrations = Integrationen
agent-section-integrations-desc = Übergeben Sie die Poly-Werkzeuge über MCP an Ihren KI-Assistenten. Kein API-Schlüssel erforderlich — Poly läuft als MCP-Server, den Sie zur Claude-App (oder einem beliebigen MCP-kompatiblen Client) hinzufügen.
agent-section-profile = Agentenprofil
agent-section-profile-desc = Ihre teilbare Visitenkarte. Agenten anderer Poly-Nutzer können Ihren Agent nach einer kurzen Vorstellung fragen, bevor sie Kontakt aufnehmen — spart Smalltalk und kommt schneller zum Punkt.
agent-profile-textarea-label = Profil
agent-profile-textarea-placeholder = z. B. „Hallo, ich bin Alex — Backend-Entwickler bei Aareon, begeistert von Rust + WASM, Kajakfahren und Indie-Horrorspielen. Freue mich immer über Gespräche über Plugin-Architekturen."
agent-profile-save = Profil speichern
agent-profile-visibility-note = Sichtbar für andere Poly-Nutzer, mit denen Ihre Konten einen Chat teilen. Wird nicht an Backends oder Dritte weitergegeben.
agent-integration-responses = Vorgeschlagene Antworten
agent-integration-responses-desc = Lassen Sie Ihren Assistenten Antworten entwerfen, die Sie vor dem Senden prüfen können.
agent-integration-summaries = Gesprächszusammenfassungen
agent-integration-summaries-desc = Lange Threads mit Zusammenfassungen auf Abruf nachholen.
agent-integration-translate = Live-Übersetzung
agent-integration-translate-desc = Eingehende Nachrichten im Handumdrehen übersetzen.
agent-integration-memory = Gedächtnis
agent-integration-memory-desc = Kontaktbezogener Kontext, den der Assistent zwischen Gesprächen beibehält.
agent-integration-outreach = Geplante Kontaktaufnahme
agent-integration-outreach-desc = „Alle N Tage pingen"-Erinnerungen planen und vom Assistenten senden lassen.
agent-integration-image-gen = Bildgenerierung
agent-integration-image-gen-desc = Den Assistenten auf Anfrage Bilder generieren und anhängen lassen.

# /agent — Antwort-Stil pro Chat (Phase E)
agent-style-title = Antwort-Stil
agent-style-tone = Ton
agent-style-tone-casual = Locker
agent-style-tone-professional = Professionell
agent-style-tone-snarky = Ironisch
agent-style-tone-warm = Herzlich
agent-style-tone-direct = Direkt
agent-style-formality = Anredeform
agent-style-formality-tu = Informal (tu / du)
agent-style-formality-vous = Formal (vous / Sie)
agent-style-formality-neutral = Neutral
agent-style-emoji = Emoji erlaubt
agent-style-signature = Signatur
agent-style-extra-notes = Weitere Hinweise
agent-style-save = Speichern

# Leerer Zustand in ServerHome, wenn der Server noch keine Kanäle hat.
server-empty-title = Noch keine Kanäle
server-empty-body = Dieser Server hat noch keine Kanäle. Bitte eine Moderation, einen Kanal anzulegen, oder erstelle selbst den ersten, wenn du die Berechtigung hast.

# Agenten-Panel
agent-panel-toggle = Agenten-Panel
agent-panel-title = Agent
agent-panel-access-label = Claude Zugriff auf diesen Chat erlauben
agent-panel-access-description = Wenn aktiv, können Tools wie get_reply_context und draft_create diesen Chat sehen und darin handeln.
agent-panel-disabled-state = Agent ist für diesen Chat deaktiviert
agent-panel-memory-title = Gedächtnis
agent-panel-memory-empty = Noch keine Fakten gespeichert.
agent-panel-memory-forget = Vergessen
agent-panel-drafts-title = Ausstehende Entwürfe
agent-panel-drafts-empty = Keine ausstehenden Entwürfe.
agent-panel-style-title = Antwortstil
agent-panel-activity-title = Letzte Aktivitäten
agent-panel-activity-empty = Noch keine Agentenaktivität.
agent-panel-activity-draft-sent = Entwurf gesendet um { $time }
agent-panel-activity-fact-remembered = Fakt gespeichert um { $time }
# Phase B — Entwurfswarteschlange (von Agenten vorgeschlagene Nachrichtenentwürfe)
agent-draft-claude-suggests = ✨ { $suggested_by } schlägt vor:
agent-draft-send = Senden
agent-draft-edit = Bearbeiten
agent-draft-discard = Verwerfen
agent-draft-autosend-in = Automatisches Senden in { $secs } s
agent-draft-cancel-autosend = Automatisches Senden abbrechen
agent-drafts-sidebar-title = Ausstehende Entwürfe
agent-drafts-sidebar-empty = Keine ausstehenden Entwürfe

# Moderationsaktionen — allgemein
mod-action-kick = Mitglied entfernen
mod-action-ban = Mitglied sperren
mod-action-unban = Sperre aufheben
mod-action-timeout = Timeout
mod-action-untimeout = Timeout aufheben
mod-action-delete-message = Nachricht löschen
mod-action-edit-channel = Kanal bearbeiten

# Backend-spezifische Überschreibungen
mod-action-discord-timeout = Timeout
mod-action-discord-ban = Sperren
mod-action-matrix-redact = Zurückziehen
mod-action-lemmy-ban = Aus Community sperren
mod-action-lemmy-timeout = Vorübergehend sperren

# Einstellungs-Tabs — Moderation
settings-tab-roles = Rollen
settings-tab-bans = Sperren
settings-tab-modlog = Protokoll

# Kick-Dialog
dialog-kick-title = { $user } vom Server entfernen?
dialog-kick-reason = Grund (optional)
dialog-kick-confirm = Entfernen

# Ban-Dialog
dialog-ban-title = { $user } sperren?
dialog-ban-reason = Grund (optional)
dialog-ban-delete-history = Nachrichtenverlauf löschen
dialog-ban-confirm = Sperren

# Timeout-Dialog
dialog-timeout-title = { $user } mit Timeout belegen
dialog-timeout-duration = Dauer
dialog-timeout-reason = Grund (optional)
dialog-timeout-confirm = Timeout
dialog-timeout-5min = 5 Minuten
dialog-timeout-10min = 10 Minuten
dialog-timeout-1hr = 1 Stunde
dialog-timeout-24hr = 24 Stunden
dialog-timeout-1week = 1 Woche

# Kanal bearbeiten
dialog-edit-channel-title = Kanal bearbeiten
dialog-edit-channel-name = Kanalname
dialog-edit-channel-topic = Thema
dialog-edit-channel-slowmode = Langsamer Modus (Sekunden, 0 = aus)
dialog-edit-channel-nsfw = NSFW / Altersbeschränkung
dialog-edit-channel-save = Speichern
dialog-cancel = Abbrechen

# Ergebnisse von Moderationsaktionen
dialog-kick-success = Mitglied entfernt.
dialog-kick-error = Entfernen fehlgeschlagen: { $error }
dialog-ban-success = Mitglied gesperrt.
dialog-ban-error = Sperren fehlgeschlagen: { $error }
dialog-timeout-success = Timeout angewendet.
dialog-timeout-error = Timeout fehlgeschlagen: { $error }
dialog-edit-channel-success = Kanal aktualisiert.
dialog-edit-channel-error = Kanalaktualisierung fehlgeschlagen: { $error }

# Sperren-Tab
bans-tab-empty = Keine Sperren vorhanden.
bans-tab-unban = Sperre aufheben
bans-tab-reason-none = (kein Grund)
bans-tab-unban-success = Sperre aufgehoben.
bans-tab-unban-error = Sperre aufheben fehlgeschlagen: { $error }
bans-tab-loading = Sperren werden geladen…

# Rollen-Tab
roles-tab-empty = Keine Rollen definiert.
roles-tab-loading = Rollen werden geladen…

# Moderationsprotokoll-Tab
modlog-tab-empty = Keine Einträge im Moderationsprotokoll.
modlog-tab-loading = Audit-Protokoll wird geladen…
modlog-tab-moderator = Moderator
modlog-tab-target = Ziel
modlog-tab-reason = Grund
modlog-action-kicked = Entfernt
modlog-action-banned = Gesperrt
modlog-action-unbanned = Sperre aufgehoben
modlog-action-timed-out = Timeout
modlog-action-role-updated = Rolle aktualisiert
modlog-action-message-deleted = Nachricht gelöscht
modlog-action-channel-updated = Kanal aktualisiert
modlog-action-other = Sonstiges: { $detail }

# Kontoübersicht-Platzhalter (Default vor Plugin-Überschreibung).
overview-default-title = Übersicht
overview-default-subtitle = Für dieses Konto wurde noch keine Übersicht definiert.

account-bar-overview-tooltip = Übersicht

overview-toggle-servers = Server
overview-toggle-dms = Direktnachrichten
overview-toggle-friends = Freunde
overview-toggle-notifications = Benachrichtigungen

# Übersichts-Subseiten
overview-page-general = Allgemein
overview-page-missed = Verpasstes
overview-page-stats = Statistik
overview-page-agents = Agenten
overview-page-missed-title = Verpasstes
overview-page-missed-subtitle = Aktuelle ungelesene Benachrichtigungen und Direktnachrichten dieses Kontos.
overview-page-stats-title = Statistik
overview-page-stats-subtitle = Deine Aktivität auf einen Blick.
overview-page-agents-title = Aktive Agenten
overview-page-agents-subtitle = Agentische Funktionen über deine Server hinweg.
overview-page-agents-empty = Noch keine Agenten aktiv. Das Agent-SDK kommt bald.
overview-empty-allcaughtup = Alles erledigt.
overview-section-unread-dms = Ungelesene Direktnachrichten
overview-section-unread-notifications = Ungelesene Benachrichtigungen
overview-stat-servers = Server
overview-stat-dms = Direktnachrichten
overview-stat-groups = Gruppen
overview-stat-unread = Ungelesen
overview-stat-mentions = Erwähnungen

overview-search-placeholder = Suchen…
