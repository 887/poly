# Poly — Français (fr) traductions principales
# Project Fluent (.ftl) format

# Application
app-title = Poly
app-description = Client de messagerie multi-plateforme

# Navigation
nav-dms = Messages directs
nav-friends = Amis
nav-notifications = Notifications
nav-settings = Paramètres
nav-servers = Serveurs
nav-demo = Basculer le client de démo
nav-demo-active = Client de démo actif

# Assistant de configuration
setup-welcome-title = Bienvenue sur Poly
setup-welcome-description = Un messager unifié pour toutes vos plateformes de chat.
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

# Salons
channel-text = Salon textuel
channel-voice = Salon vocal
channel-video = Salon vidéo

# Utilisateurs
user-online = En ligne
user-idle = Absent
user-dnd = Ne pas déranger
user-invisible = Invisible
user-offline = Hors ligne
user-members = Membres

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
account-switch = Changer de compte
account-settings = Paramètres du compte
settings-account-settings = Paramètres du compte
settings-backup = Serveurs de sauvegarde
settings-backup-description = Configurer les serveurs de synchronisation chiffrée
settings-add-backup = Ajouter un serveur de sauvegarde
settings-identity = Identité
settings-identity-description = Votre identité Poly et options de récupération
settings-your-id = Votre identifiant de compte
settings-export-recovery = Exporter la phrase de récupération
settings-theme = Thème
settings-theme-description = Personnaliser les couleurs, thèmes et l'apparence
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
settings-general = Général
settings-general-description = Préférences de notification et comportement au démarrage
settings-reset-description = Réinitialisez les données de l’app pour repartir à zéro, ou détruisez tout l’état local pour des tests propres.
settings-reset-app = Réinitialiser les données de l’app
settings-nuke-app = NUKER l’état de l’app
settings-reset-error-no-storage = Le stockage n'est pas encore prêt
settings-reset-error-failed = Échec de la réinitialisation des données de l’app
settings-nuke-error-failed = Échec du NUKING de l’état de l’app
settings-reset-error-reload = Réinitialisation réussie, mais rechargement échoué

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
action-search = Rechercher
action-copy = Copier
action-back = Retour
action-confirm = Confirmer

# Erreurs
error-generic = Quelque chose s'est mal passé. Veuillez réessayer.
error-network = Erreur réseau. Vérifiez votre connexion.
error-auth-failed = Échec de l'authentification. Veuillez vérifier vos identifiants.
error-not-found = Non trouvé.

# Voix / Vidéo
voice-connected = Voix connectée
voice-join-voice = Rejoindre la voix
voice-join-video = Rejoindre la vidéo
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
voice-go-to-channel = Aller au salon
voice-mute-mic = Couper le microphone
voice-unmute-mic = Activer le microphone
voice-camera = Activer la caméra
voice-screen-share = Partager l'écran
voice-activity = Partager une activité
voice-voiceboard = Tableau vocal
voice-signal-quality = Qualité du signal

# Emoji / GIF / Réactions
emoji-picker = Emoji
emoji-search = Chercher un emoji...
gif-picker = GIF
reaction-add = Ajouter une réaction
chat-drop-files = Déposez des fichiers pour les envoyer
chat-attach-file = Joindre un fichier

# Navigation
nav-back = Retour
nav-forward = Avancer

# Settings search
settings-search = Rechercher dans les paramètres...
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
dm-search-placeholder = Trouver ou démarrer une conversation
dm-no-results = Aucune conversation trouvée

# Friends panel
friends-title = Amis
friends-search-placeholder = Rechercher des amis...
friends-none = Aucun ami trouvé
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
chat-select-channel = Sélectionnez un salon pour commencer à discuter
chat-timestamp-yesterday = Hier { $time }

# Utilisateurs extras
user-no-members = Aucun membre à afficher
account-not-signed-in = Non connecté

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
