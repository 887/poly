# Poly — Français (fr) traductions principales
# Project Fluent (.ftl) format

# Application
app-title = Poly
electron-window-minimize = Réduire
electron-window-maximize = Agrandir ou restaurer
electron-window-close = Fermer la fenêtre
app-description = Client de messagerie multi-plateforme
wasm-crash-title = Poly a subi un crash navigateur
wasm-crash-description = La page actuelle a planté ou a déclenché une erreur navigateur/WASM non gérée. L’interface sous cette surcouche n’est plus fiable.
wasm-crash-details-label = Type de crash
wasm-crash-location-label = Emplacement source
wasm-crash-path-label = Route
wasm-crash-reload-action = Recharger Poly
wasm-crash-kind-panic = Panic Rust
wasm-crash-kind-window-error = Événement d’erreur du navigateur
wasm-crash-kind-unhandled-rejection = Rejet de promesse non géré
wasm-crash-kind-unknown = Crash inconnu
wasm-crash-generic-message = Le navigateur n’a fourni aucun détail sur le crash.
wasm-crash-window-error-fallback = Le navigateur a signalé un événement d’erreur global sans message.
wasm-crash-rejection-fallback = Une promesse a été rejetée sans message d’erreur lisible.

# Navigation
nav-dms = Messages directs
nav-friends = Amis
nav-notifications = Notifications
nav-settings = Paramètres
nav-search = Recherche
nav-servers = Serveurs
nav-demo = Basculer le client de démo
nav-demo-active = Client de démo actif

# Assistant de configuration
setup-welcome-title = Bienvenue sur Poly
setup-welcome-description = Un messager unifié pour toutes vos plateformes de chat.
setup-welcome-tagline = Votre couche sociale alimentée par l'IA. Tous vos chats unifiés, avec un agent IA qui se souvient, répond et gère vos conversations.
setup-feature-plugins = Basé sur des plugins — Discord, Matrix, Teams, Stoat et plus via des plugins WASM
setup-feature-multi-account = Boîte de réception unifiée — tous vos comptes sur toutes les plateformes en un seul endroit
setup-feature-demo = Données démo chargées — explorez l'application avec des conversations d'exemple
setup-feature-keys = Clés d'identité — générez-les dans Paramètres → Identité quand vous êtes prêt
setup-feature-ai = Agent IA — résumés de chats, réponses automatiques et n'oubliez jamais une conversation
setup-feature-translate = Traduction en direct — traduisez les messages à la volée dans n'importe quelle langue
setup-get-started = Commencer
setup-generating-keys = Génération de vos clés d'identité...
setup-your-account-id = Votre identifiant de compte
setup-account-id-description = C'est votre identifiant unique. Partagez-le avec vos amis pour vous connecter.
setup-recovery-phrase = Phrase de récupération
setup-recovery-phrase-description = Notez ces mots et conservez-les en lieu sûr. Vous en aurez besoin pour récupérer votre compte.
setup-recovery-warning = Si vous perdez votre phrase de récupération, vous perdrez définitivement l'accès à votre compte.
setup-copy-phrase = Copier la phrase
setup-export-phrase = Exporter dans un fichier
setup-confirm-phrase = Confirmer la phrase de récupération
setup-confirm-description = Entrez les mots de votre phrase de récupération pour confirmer que vous les avez sauvegardés.
setup-continue = Continuer
setup-skip-confirmation = Passer la confirmation
setup-complete = Configuration terminée
setup-complete-description = Votre identité a été créée. Ajoutez des comptes de messagerie dans les Paramètres.
setup-go-to-app = Aller sur Poly

# Chat
chat-type-message = Tapez un message...
chat-send = Envoyer
chat-typing = { $user } est en train d'écrire...
chat-typing-multiple = { $count } personnes sont en train d'écrire...
chat-no-messages = Aucun message pour le moment. Lancez la conversation !
chat-load-more = Charger plus
chat-edited = (modifié)
chat-loading = Chargement des messages...
chat-select-conversation = Sélectionnez une conversation
chat-loading-earlier = Chargement des anciens messages...
chat-unread-banner = { $count } nouveaux messages depuis { $time } le { $date }
chat-unread-divider = Nouveau
chat-jump-to-present = Aller au présent
chat-viewing-older-messages = Vous regardez des messages plus anciens

# Salons
channel-text = Salon textuel
channel-voice = Salon vocal
channel-video = Salon vidéo

# Utilisateurs / Statut
user-online = En ligne
user-idle = Absent
user-dnd = Ne pas déranger
user-invisible = Invisible
user-offline = Hors ligne
user-away = Absent
user-appear-offline = Apparaître hors ligne
user-members = Membres
user-no-members = Aucun membre

# Barre de compte — badges coins d'avatar
account-profile-click-hint = Cliquez pour voir votre profil
account-conn-connected = Connecté
account-conn-connecting = Connexion…
account-conn-disconnected = Hors ligne
account-conn-error = Erreur de connexion

# Popup de sélection de statut
status-picker-title = Définir le statut

# Filtre de la liste des membres
member-filter-placeholder = Rechercher des membres…
member-filter-tooltip = Rechercher des membres
member-filter-no-results = Aucun membre ne correspond à cette recherche.

# User profile modal
user-profile-more-options = Plus d'options
user-profile-message = Message
user-profile-call = Appel
user-profile-video = Vidéo
user-profile-add-to-call = Ajouter à l'appel
user-profile-add-video-to-call = Ajouter à l'appel vidéo
user-profile-note = Note
user-profile-note-placeholder = Cliquer pour ajouter une note
user-profile-open = Voir le profil

# Notifications
notifications-title = Notifications
notifications-empty = Aucune nouvelle notification
notifications-mark-read = Marquer comme lu
notifications-dismiss = Fermer
notifications-mention = { $user } vous a mentionné dans { $channel }
notifications-friend-request = { $user } vous a envoyé une demande d'ami
notifications-server-invite = Vous avez été invité à { $server }

# Paramètres
settings-title = Paramètres
settings-accounts = Comptes
settings-accounts-description = Gérer vos comptes de messagerie
settings-add-account = Ajouter un compte
settings-remove-account = Supprimer le compte
settings-no-accounts = Aucun compte connecté. Ajoutez un compte pour commencer.
settings-account-settings-link = Paramètres du compte
account-switch = Changer de compte
account-settings = Paramètres du compte
settings-account-settings = Paramètres du compte

# Flux d'inscription — sélection du backend
signup-picker-title = Ajouter un compte
signup-picker-description = Choisissez quel type de compte ajouter.
signup-picker-back = ← Retour aux paramètres
signup-stub-back = ← Choisir un backend
# ── Créer serveur / canal ────────────────────────────────────────────────────
create-server-btn = Créer un serveur
create-server-placeholder = Nom du serveur…
create-server-submit = Créer
create-server-cancel = Annuler
create-server-creating = Création…
create-server-page-title = Créer un serveur
create-server-page-subtitle = Donnez un nom à votre serveur. Vous pourrez le modifier à tout moment.
create-server-page-label = Nom du serveur
channel-list-text-channels = Salons textuels
create-channel-btn = Nouveau canal
create-channel-page-title = Créer un canal
create-channel-page-subtitle = Donnez un nom à votre canal. Vous pourrez le modifier à tout moment.
create-channel-page-label = Nom du canal
create-channel-placeholder = Nom du canal…
create-channel-submit = Créer
create-channel-cancel = Annuler
create-channel-creating = Création…
settings-backup = Serveurs de sauvegarde
settings-backup-description = Configurer les serveurs de synchronisation chiffrée
settings-add-backup = Ajouter un serveur de sauvegarde
settings-identity = Identité
settings-identity-description = Votre identité d’appareil, votre phrase de récupération et où cette identité est utilisée
settings-your-id = Votre identifiant de compte
settings-export-recovery = Exporter la phrase de récupération
settings-theme = Thème
settings-theme-description = Personnaliser les couleurs, thèmes et l'apparence
settings-media = Médias
settings-media-description = Configurez les fournisseurs GIF et les futures intégrations multimédia
settings-media-active-provider = Fournisseur GIF actif
settings-media-api-key = Clé API
settings-media-api-key-placeholder = Collez la clé API du fournisseur
settings-media-provider-klippy = Klippy
settings-media-provider-giphy = Giphy
settings-media-provider-imgur = Imgur
settings-media-status-configured = Configuré
settings-media-status-not-setup = Non configuré
settings-theme-preset = Préréglage de thème
settings-theme-custom-css = CSS personnalisé
settings-theme-import = Importer un thème
settings-theme-export = Exporter un thème
settings-color-mode = Mode de couleur
settings-color-overrides = Personnalisation des couleurs
settings-color-hint = Activez cette option pour remplacer les couleurs individuelles du thème. Désactivez pour revenir au thème par défaut.
settings-reset-colors = Réinitialiser les couleurs
settings-theme-apply-css = Appliquer le CSS
settings-css-hint = Décommentez une variable pour remplacer le thème. L’interrupteur active/désactive ces modifications CSS.
settings-css-reset-template = Réinitialiser le modèle
settings-language = Langue
settings-language-description = Choisissez votre langue préférée
settings-appearance = Apparence
settings-appearance-description = Mode sombre, mode clair et options d'affichage
settings-dark-mode = Mode sombre
settings-light-mode = Mode clair
settings-follow-device = Suivre la préférence de l'appareil
settings-layout = Disposition
settings-layout-description = Comportement de disposition et miroir sur les interfaces bureau et mobile
settings-general = Général
settings-general-description = Réinitialisez les données locales de l’app ou détruisez complètement l’état pour des re-tests propres
settings-layout-mode = Mode de disposition
settings-layout-mode-description = Choisissez si Poly doit détecter le mode mobile par largeur, par orientation portrait, ou toujours forcer bureau/mobile. Les overrides d’URL comme ?layout=mobile ou ?layout=desktop priment tant qu’ils sont présents.
settings-layout-auto-width = Auto (largeur ≤ 640px)
settings-layout-auto-portrait = Auto (portrait)
settings-layout-force-desktop = Forcer le bureau
settings-layout-force-mobile = Forcer le mobile
settings-mirror-menu-layout = Miroir des menus / volets
settings-mirror-menu-layout-description = Échange les volets gauche et droit sur bureau et mobile, y compris l’ordre des barres latérales et les boutons de l’en-tête mobile.
settings-mirror-chat-messages = Miroir des messages du chat
settings-mirror-chat-messages-description = Place les avatars / gouttières de message à droite tout en gardant le texte lisible.
settings-force-mobile-layout = Forcer la disposition mobile
settings-force-mobile-layout-description = Utilise l’interface mobile même au-dessus de 640px. Désactive cette option pour garder l’interface bureau jusqu’à ce que la fenêtre devienne naturellement étroite.
settings-reset-description = Réinitialisez les données de l’app pour repartir à zéro, ou détruisez tout l’état local pour des tests propres.
settings-reset-app = Réinitialiser les données de l’app
settings-nuke-app = NUKER l’état de l’app
settings-reset-error-no-storage = Le stockage n'est pas encore prêt
settings-reset-error-failed = Échec de la réinitialisation des données de l’app
settings-nuke-error-failed = Échec du NUKING de l’état de l’app
settings-reset-error-reload = Réinitialisation réussie, mais rechargement échoué

# Paramètres Démo
settings-demo = Démo
settings-demo-description = Gérer le client de données démo intégré. Lorsqu’il est activé, Poly charge des comptes d’exemple avec des serveurs, des canaux et des conversations pour explorer l’application.
settings-demo-toggle = Activer les données démo

# Gestionnaire de plugins
settings-plugins = Plugins
settings-plugins-description = Activer ou désactiver les plugins de backend de messagerie. Chaque plugin est un client de messagerie. Les comptes sont des sessions créées par ces plugins.
plugins-native-title = Plugins intégrés
plugins-native-description = Ces plugins sont compilés dans cette version de Poly. Activez-les ou désactivez-les via les cases à cocher.
plugins-loaded-count = Backends actifs
plugins-none-loaded = Aucun plugin WASM ajouté. Ajoutez une URL de plugin ci-dessous.
plugins-status-disconnected = Déconnecté
plugins-status-connecting = Connexion…
plugins-status-connected = Connecté
plugins-status-error = Erreur
plugins-type-native = Natif
plugins-type-wasm = WASM
plugins-not-compiled = absent de ce build
plugins-active-accounts = Comptes actifs
plugins-wasm-title = Plugins WASM
plugins-wasm-description = Les plugins WASM étendent Poly avec des backends supplémentaires. Chargez un plugin depuis une URL — Poly ajoutera automatiquement la version WIT.
plugins-add-wasm-title = Ajouter un plugin depuis une URL
plugins-add-wasm-description = Entrez l'URL de base d'un plugin WASM. La version WIT sera ajoutée automatiquement.
plugins-url-placeholder = https://plugins.example.com/matrix.wasm
plugins-name-placeholder = Nom d'affichage (optionnel)
plugins-add-btn = Ajouter le plugin
plugins-url-required = Veuillez entrer une URL de plugin
plugins-remove = Supprimer
plugins-wit-hint = Version de l'interface WIT

# Paramètres des plugins
settings-plugin-settings = Paramètres des plugins
# Libellé affiché avant les sections fournies par les plugins dans la barre de navigation
settings-plugins-section-divider = Paramètres des plugins
# En-tête de groupe dans la barre latérale séparant les sections intégrées des pages de plugins
settings-plugin-settings-nav-header = Paramètres des plugins
# Petit badge pour les sections fournies par les plugins
settings-plugins-badge = Plugin
plugin-settings-nav-title = Paramètres des backends
plugin-settings-none = Aucun backend avec des paramètres n'est chargé. Activez les données démo ou connectez un compte.
plugin-settings-generic-description = Ce backend n'a pas encore de paramètres personnalisés. Les paramètres apparaîtront ici lorsque le plugin les prendra en charge.
# Note : les chaînes plugin-demo-* sont chargées depuis le bundle FTL du plugin démo.

# Backup Server Settings
settings-backup-add-server = Ajouter un serveur
settings-backup-url-placeholder = http://127.0.0.1:8080
settings-backup-url-label = URL du serveur
settings-backup-label-label = Nom du serveur
settings-backup-passphrase-label = Phrase secrète du serveur
settings-backup-connect = Connecter
settings-backup-connecting = Connexion...
settings-backup-cancel = Annuler
settings-backup-status-unknown = Inconnu
settings-backup-status-connected = Connecté
settings-backup-status-auth-required = Authentification requise
settings-backup-status-unreachable = Inaccessible
settings-backup-status-syncing = Synchronisation...
settings-backup-sync-now = Synchroniser maintenant
settings-backup-reauth = Ré-authentifier
settings-backup-remove = Supprimer
settings-backup-last-synced = Dernière sync: { $time }
settings-backup-never-synced = Jamais synchronisé
settings-backup-enabled = Activé
settings-backup-auth-success = Connecté!
settings-backup-auth-failed = Échec de l'authentification
settings-backup-no-servers = Aucun serveur de sauvegarde configuré.
settings-backup-wizard-step1 = URL du serveur
settings-backup-wizard-step2 = Connecter
settings-backup-step1-hint = Entrez l'URL de votre serveur de sauvegarde Poly
settings-backup-step2-hint = Donnez un nom et entrez les identifiants pour terminer
settings-backup-check-btn = Vérifier la connexion
settings-backup-checking = Vérification…
settings-backup-continue = Continuer
settings-backup-back = Retour
settings-backup-finish = Terminer la configuration
settings-backup-url-empty = Veuillez entrer une URL de serveur
settings-backup-password-required = 🔒 Mot de passe requis
settings-backup-no-password-required = ✓ Aucun mot de passe requis
settings-backup-server-full = Serveur plein — inscriptions désactivées

# Identity Settings
settings-identity-your-id-label = Votre identifiant de compte Poly
settings-identity-copy-id = Copier l'ID
settings-identity-show-phrase = Afficher la phrase de récupération
settings-identity-phrase-modal-title = Votre phrase de récupération
settings-identity-phrase-warning = Gardez cette phrase secrète. Quiconque la possède peut accéder à votre compte.
settings-identity-copy-all = Copier tous les mots
settings-identity-close = Fermer
settings-identity-no-identity = Identité pas encore générée. Terminez d'abord l'assistant de configuration.
settings-identity-create-btn = Créer une identité
settings-identity-creating = Création…
settings-identity-purpose = Cette identité correspond au matériel de clé que Poly utilise pour vous :
settings-identity-purpose-poly = Les serveurs Poly l’utilisent pour la connexion par clé et les fonctionnalités chiffrées de bout en bout.
settings-identity-purpose-backup = Les serveurs de sauvegarde l’utilisent pour dériver les clés de chiffrement et authentifier la synchronisation chiffrée.
settings-identity-backup-servers = Serveurs de sauvegarde
settings-identity-backup-servers-description = Cette identité est utilisée pour l'authentification sur les serveurs de sauvegarde suivants.
settings-identity-poly-accounts = Comptes Poly Server
settings-identity-poly-accounts-description = Cette identité est utilisée pour les comptes suivants sur les serveurs Poly auto-hébergés.
settings-identity-no-servers = Aucun serveur de sauvegarde configuré pour le moment.
settings-identity-no-poly-accounts = Aucun compte Poly server.
settings-identity-status-active = Actif
settings-identity-status-disabled = Désactivé
settings-identity-delete = Supprimer l'identité
settings-identity-delete-confirm-title = Supprimer l'identité ?
settings-identity-delete-confirm-message = Cela supprimera définitivement cette clé d'identité. Assure-toi d'avoir sauvegardé la phrase de récupération sinon tu ne pourras pas récupérer l'accès !
settings-identity-delete-confirm = Oui, supprimer
settings-identity-cancel = Annuler

# Préréglages de thème
theme-blue = Bleu
theme-purple = Violet
theme-red = Rouge
theme-green = Vert
theme-monotone = Monotone

# Backends
backend-stoat = Stoat
backend-matrix = Matrix
backend-discord = Discord
backend-teams = Teams
backend-demo = Démo

# Actions communes
action-save = Enregistrer
action-cancel = Annuler
action-delete = Supprimer
action-edit = Modifier
action-close = Fermer
action-more = Plus
chat-replying-to = Répondre à { $name }
action-search = Rechercher
action-copy = Copier
action-back = Retour
action-confirm = Confirmer
action-clear = Effacer
action-download = Télécharger
action-open-in-browser = Ouvrir dans le navigateur
zoom-in = Zoomer
zoom-out = Dézoomer

media-viewer-unavailable-title = Média indisponible
media-viewer-unavailable-body = Ce média n'a pas pu être chargé à partir de l'état actuel du chat.

# Erreurs
error-generic = Quelque chose s'est mal passé. Veuillez réessayer.
error-network = Erreur réseau. Vérifiez votre connexion.
error-auth-failed = Échec de l'authentification. Veuillez vérifier vos identifiants.
error-not-found = Non trouvé.

# Voix / Vidéo
voice-connected = Voix connectée
voice-join-voice = Rejoindre la voix
voice-join-video = Rejoindre la vidéo
voice-direct-call = Appel direct
voice-group-call = Appel de groupe
voice-swap-held-call = Reprendre l'appel en attente
voice-disconnect = Déconnecter
voice-muted = Micro coupé
voice-deafened = Son coupé
voice-streaming = Partage d'écran
voice-video-on = Caméra activée
voice-mute = Couper le micro
voice-unmute = Activer le micro
voice-deafen = Couper le son
voice-undeafen = Activer le son
voice-no-channel = Aucun canal sélectionné
voice-no-one-here = Personne n'est ici
voice-be-first = Soyez le premier à rejoindre !
voice-watching-screen = Visionnage du partage d'écran
voice-in-channel = dans le salon
voice-in-call = dans l'appel
voice-go-to-channel = Aller au salon
voice-go-to-conversation = Aller à la conversation
direct-call-calling = Appel en cours…
direct-call-calling-video = Démarrage de l'appel vidéo…
direct-call-adding = Ajout à l'appel…
direct-call-adding-video = Ajout vidéo à l'appel…
direct-call-awaiting-join = En attente de la connexion à l'appel
direct-call-ringing = Ça sonne… appuyez sur × pour annuler
direct-call-cancel = Annuler l'appel
voice-mute-mic = Couper le microphone
voice-unmute-mic = Activer le microphone
voice-camera = Activer la caméra
voice-screen-share = Partager l'écran
mobile-nav-open = Ouvrir le menu de navigation
mobile-nav-close = Fermer le menu de navigation
voice-activity = Partager une activité
voice-voiceboard = Tableau vocal
voice-signal-quality = Qualité du signal
voice-stop-camera = Arrêter la caméra
voice-stop-share = Arrêter le partage
voice-camera-preview = Aperçu caméra
voice-screen-sharing = Aperçu du partage d'écran
voice-audio-settings = Paramètres voix & audio
voice-mic-device = Périphérique d'entrée (Microphone)
voice-speaker-device = Périphérique de sortie (Haut-parleur)
voice-default-device = Défaut
voice-noise-cancel = Réduction du bruit
voice-noise-cancel-desc = Supprimez les bruits de fond via réduction IA (RNNoise).
voice-noise-cancel-on = Réduction du bruit : Activée
voice-noise-cancel-off = Réduction du bruit : Désactivée
voice-server-location = Emplacement du serveur
voice-testing-mic = Test en cours... (3s)
voice-test-mic = Tester le microphone (3 sec)

# Emoji / GIF / Réactions
emoji-picker = Emoji
emoji-search = Chercher un emoji...
gif-picker = GIF
stickers-picker = Autocollants
media-picker-gif-placeholder = Recherche de GIF bientôt disponible
media-picker-stickers-placeholder = Autocollants bientôt disponibles
media-picker-markdown = Mise en forme Markdown
reaction-add = Ajouter une réaction

# Barre d'actions de message / menu contextuel
msg-reply = Répondre
msg-forward = Transférer
msg-edit = Modifier
msg-delete = Supprimer
msg-copy-text = Copier le texte
msg-apps = Applications
msg-mark-unread = Marquer comme non lu
msg-copy-link = Copier le lien du message
msg-speak = Dicter le message
msg-report = Signaler le message
msg-copy-id = Copier l'ID du message
msg-edit-save = Enregistrer
msg-edit-cancel = Annuler

chat-drop-files = Déposez des fichiers pour les envoyer
chat-attach-file = Joindre un fichier

# Navigation
nav-back = Retour
nav-forward = Avancer

# Settings search
settings-search = Rechercher dans les paramètres...
settings-search-no-results = Aucun paramètre trouvé pour cette recherche.
settings-search-found = Paramètres Trouvés
settings-voice-video = Voix & Vidéo
settings-notifications = Notifications
account-settings-title = Paramètres du compte

# Voice & Video settings
voice-input-device = Périphérique d'entrée
voice-output-device = Périphérique de sortie
voice-input-volume = Volume d'entrée
voice-output-volume = Volume de sortie
voice-mic-test = Tester le micro
voice-mic-test-stop = Arrêter le test
voice-input-mode = Mode d'entrée
voice-input-vad = Détection d'activité vocale
voice-input-ptt = Appuyer pour parler
voice-noise-suppression = Suppression du bruit
voice-noise-off = Désactivé
voice-noise-standard = Standard
voice-noise-high = Élevé
voice-echo-cancel = Annulation d'écho

# Notifications settings
notif-enable-desktop = Activer les notifications bureau
notif-permission-request = Autoriser les notifications
notif-global-header = Global (Appareil)
notif-notify-about = Me notifier pour
notif-sounds = Sons
notif-badges = Badges
notif-streams = Des personnes que je connais diffusent
notif-friends-voice = Des amis rejoignent des canaux vocaux
notif-reactions = Quelqu'un réagit à mes messages
notif-sounds-new-message = Nouveau message
notif-sounds-dm = Messages directs
notif-sounds-ring = Appel entrant
notif-badge-unread = Activer le badge messages non lus
notif-no-accounts = Aucun compte actif. Ajoutez un compte dans Paramètres → Comptes.

# DM list
dm-saved-messages = Messages enregistrés
dm-new-conversation = Nouvelle conversation
dm-search-conversations = Rechercher des conversations
dm-search-placeholder = Trouver ou démarrer une conversation
saved-items-title = Messages enregistrés
saved-items-description = Revenez aux messages épinglés de vos messages privés et groupes.
saved-items-empty = Aucun message épinglé pour le moment.
saved-items-all-sources = Toutes les sources
saved-items-filter-placeholder = Filtrer les sources enregistrées...
saved-items-sources-empty = Aucune source enregistrée trouvée
dm-no-results = Aucune conversation trouvée

# Friends panel
friends-title = Amis
friends-management-title = Personnes
friends-management-description = Gérez les amis, les utilisateurs ignorés et les utilisateurs bloqués pour ce compte.
friends-management-message = Envoyer un message
friends-ignored-title = Ignorés
friends-ignored-empty = Aucun utilisateur ignoré pour le moment.
new-conversation-description = Choisissez un ami pour démarrer une conversation directe. Les conversations à plusieurs utiliseront ce composeur dès que la création de groupe partagée sera connectée.
new-conversation-start-dm = Démarrer la conversation
new-conversation-group-pending = Les conversations à plusieurs arrivent ensuite.
conversation-search-title = Rechercher des conversations
conversation-search-description = Recherchez les messages privés et groupes pour { $account }.
friends-search-placeholder = Rechercher des amis...
friends-none = Aucun ami trouvé
notifications-filter-all-types = Toutes les notifications
notifications-filter-mentions = Mentions
notifications-filter-friend-requests = Demandes d'ami
notifications-filter-server-invites = Invitations au serveur
notifications-filter-voice-invites = Invitations vocales
notifications-filter-other = Autres
notifications-unread-count = non lues
filter-all = Tous les comptes
filter-all-servers = Tous les serveurs

# Formatage du temps
time-just-now = à l'instant
time-one-minute-ago = il y a 1 minute
time-minutes-ago = il y a { $count } minutes
time-one-hour-ago = il y a 1 heure
time-hours-ago = il y a { $count } heures
time-one-day-ago = il y a 1 jour
time-days-ago = il y a { $count } jours

# Chat extras
chat-toggle-members = Afficher/masquer la liste des membres
chat-toggle-contact = Afficher/masquer les infos du contact
chat-select-channel = Sélectionnez un salon pour commencer à discuter
chat-timestamp-yesterday = Hier { $time }
search-messages = Rechercher des messages
search-placeholder = Rechercher dans ce salon...
search-placeholder-channel = Rechercher dans #{ $channel }
search-placeholder-user = Rechercher { $user }
search-placeholder-group = Rechercher { $group }
search-results = Résultats
search-no-results = Aucun message ne correspond à cette recherche
search-filter-from-user = D'une personne précise
search-filter-from-user-subtitle = de : utilisateur
search-filter-in-channel = Envoyé dans un salon précis
search-filter-in-channel-subtitle = dans : salon
search-filter-has-link = Inclut un type de donnée précis
search-filter-has-link-subtitle = contient : lien, intégration ou fichier
search-filter-mentions = Mentionne une personne précise
search-filter-mentions-subtitle = mentions : utilisateur
search-filter-more = Plus de filtres
search-filter-more-subtitle = dates, type d'auteur et plus encore
pinned-messages = Messages épinglés
no-pinned-messages = Aucun message épinglé
threads = Fils
no-threads = Aucun fil pour le moment
chat-notifications = Notifications
chat-no-notifications = Aucune notification ici
chat-type-message-channel = Envoyer un message dans #{ $channel }
chat-type-message-user = Envoyer un message à { $user }
chat-type-message-group = Envoyer un message à { $group }
chat-markdown-formatting = Mise en forme Markdown

# Utilisateurs extras
user-no-members = Aucun membre à afficher
user-all-offline-hidden = Tous les membres sont hors ligne et masqués
account-not-signed-in = Non connecté

# Paramètres du chat — liste des membres
chat-settings-member-list = Liste des membres
chat-settings-grouping = Regroupement
chat-settings-grouping-by-status = Par statut
chat-settings-grouping-none = Aucun regroupement
chat-settings-sort-order = Ordre de tri
chat-settings-sort-alphabetical = Alphabétique
chat-settings-sort-online-first = En ligne d'abord
chat-settings-sort-join-order = Ordre d'arrivée
chat-settings-show-offline = Afficher les membres hors ligne

# Libellés de couleur
color-accent = Accent
color-background = Arrière-plan
color-surface = Surface
color-text = Texte
color-secondary-text = Texte secondaire
color-border = Bordure
color-favorites-bar = Arrière-plan barre favoris
color-account-bar = Arrière-plan barre comptes

# Périphériques audio par défaut
voice-default-mic = Microphone par défaut
voice-default-speakers = Haut-parleurs par défaut

# Messages d'erreur
error-storage-unavailable = Stockage non disponible
error-load-settings = Échec du chargement des paramètres
error-reload-servers = Échec du rechargement des serveurs

# Server context menu
server-menu-mark-read = Marquer comme lu
server-menu-invite = Inviter sur le serveur
server-menu-unmute = Réactiver le son du serveur
server-menu-mute = Mettre le serveur en sourdine
server-menu-notif-settings = Paramètres de notification
server-menu-hide-muted = Masquer les canaux muets
server-menu-show-all = Afficher tous les canaux
server-menu-privacy = Paramètres de confidentialité
server-menu-edit-profile = Modifier le profil du serveur
server-menu-leave = Quitter le serveur
server-menu-copy-id = Copier l'ID du serveur
server-menu-add-favorites = Ajouter aux favoris
server-menu-remove-favorites = Supprimer des favoris

# Remove from favorites inline confirm
remove-favorites-title = Supprimer « { $name } » des favoris ?
remove-favorites-body = Vous pouvez le rajouter à tout moment en le faisant glisser vers la barre des favoris ou en utilisant ce menu.
remove-favorites-cancel = Annuler
remove-favorites-confirm = Supprimer

# Menu déroulant de la bannière du serveur
server-banner-settings = Paramètres du serveur
server-banner-invite = Inviter des personnes
server-banner-notif-settings = Paramètres de notification
server-banner-create-channel = Créer un canal
server-banner-channels-roles = Salons et rôles
server-banner-browse-channels = Parcourez les salons et activez les catégories de ce serveur.
server-banner-channel-count = salons
server-banner-leave = Quitter le serveur

# Server settings page
# Paramètres du serveur
server-settings-title = Paramètres du serveur
server-settings-overview = Vue d'ensemble
server-settings-notifications = Notifications
server-settings-profile = Profil
server-settings-general = Général

# Vue d'ensemble du serveur (icône + bannière)
server-overview-icon = Icône du serveur
server-overview-icon-url = URL de l'icône
server-overview-icon-hint = URL de l'image de l'icône. SVG ou PNG au format carré recommandé.
server-overview-banner = Bannière du serveur
server-overview-banner-url = URL de la bannière
server-overview-banner-hint = URL de la grande image de bannière affichée au-dessus de la liste des canaux. Format paysage (ex. 960×240) recommandé.
server-overview-save = Enregistrer
server-overview-saved = Enregistré
server-overview-local-override = Remplacer l'icône localement
server-overview-local-override-hint = Ce backend ne prend pas en charge les icônes de serveur personnalisées. L'icône définie ici est stockée uniquement sur cet appareil.

# Leave server inline confirm
leave-server-title = Quitter « { $name } » ?
leave-server-body = Vous ne pourrez rejoindre que si vous êtes réinvité.
leave-server-cancel = Annuler
leave-server-confirm = Quitter le serveur

# Server notification settings
server-notif-all = Tous les messages
server-notif-mentions = Seulement les @mentions
server-notif-nothing = Rien
server-notif-suppress-everyone = Supprimer @everyone et @here
server-notif-suppress-roles = Supprimer toutes les @mentions de rôle
server-notif-suppress-highlights = Supprimer les mises en avant
server-notif-mute-events = Muet pour les nouveaux événements
server-notif-mobile-push = Notifications push mobiles

# Server profile settings
server-profile-nickname = Pseudo sur le serveur
server-profile-nickname-hint = Changez comment vous apparaissez sur ce serveur
server-profile-save = Enregistrer les modifications

# Server general settings
server-general-info = Infos du serveur
server-general-danger = Zone de danger

# Group DMs
group-members-title = Membres
group-member-remove = Retirer
group-member-remove-tooltip = Retirer { $name } de ce groupe

# DM header
dm-header-subtitle = Message direct

# Presence status labels
presence-online = En ligne
presence-away = Absent
presence-dnd = Ne pas déranger
presence-offline = Hors ligne

# DM contact panel
dm-contact-panel-title = Infos du contact
dm-contact-not-found = Contact introuvable

# Demo backend
demo-regenerate-data = Régénérer les données démo

# Search page
search-page-title = Recherche
search-page-placeholder = Rechercher serveurs, canaux, DMs, groupes…
search-page-accounts = Comptes
search-page-dms = Messages Directs
search-page-groups = Groupes
search-page-type-filter = Afficher
search-type-servers = Serveurs
search-type-dms = DMs
search-type-groups = Groupes
search-showing-of = { $count } sur { $total } affichés
search-load-more = Défilez pour en voir plus…

# Context menus (shared items)
menu-copy-text = Copy text
menu-copy-id = Copy ID
menu-view-profile = View profile
