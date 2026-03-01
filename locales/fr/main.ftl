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

# Backup Server Settings
settings-backup-add-server = Ajouter un serveur
settings-backup-url-placeholder = https://backup.example.com
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
