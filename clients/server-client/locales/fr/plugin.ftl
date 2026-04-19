# Poly Server client plugin — French translations
# All keys MUST be prefixed with "plugin-poly-"

# --- Signup picker card ---
plugin-poly-signup-name = Poly Server
plugin-poly-signup-desc = Connectez-vous à un serveur Poly auto-hébergé.

# --- Signup page ---
plugin-poly-signup-title = Ajouter un compte Poly Server
plugin-poly-signup-description = Entrez l'URL de votre serveur pour vous connecter. Poly utilisera votre clé d'identité d'appareil pour vous connecter, et les nouveaux comptes Poly Server nécessitent aussi une adresse e-mail.
plugin-poly-signup-back = ← Choisir le backend
plugin-poly-signup-url-label = URL du serveur
plugin-poly-signup-url-placeholder = http://127.0.0.1:7080

# --- Étape 1 : URL + Connexion ---
plugin-poly-connect-btn = Connexion

# --- Étape 2 : Formulaire d'inscription ---
plugin-poly-signup-no-account-desc = Aucun compte n'a été trouvé pour votre clé d'identité sur ce serveur. Choisissez un nom d'utilisateur et une adresse e-mail pour créer un nouveau compte.
plugin-poly-existing-accounts-desc = Ce serveur possède déjà un ou plusieurs comptes liés à votre clé d'identité. Choisissez-en un pour vous connecter, ou créez un autre compte sur ce serveur.
plugin-poly-signup-another-account-desc = Cette clé d'identité est déjà liée à d'autres comptes sur ce serveur. Choisissez un nom d'utilisateur et une adresse e-mail pour créer un compte supplémentaire.
plugin-poly-signup-username-label = Nom d'utilisateur
plugin-poly-signup-username-placeholder = alice
plugin-poly-signup-email-label = Adresse e-mail
plugin-poly-signup-email-placeholder = alice@example.com
plugin-poly-signup-displayname-label = Nom d'affichage
plugin-poly-signup-displayname-placeholder = Alice
plugin-poly-create-account-btn = Créer un compte
plugin-poly-create-another-account-btn = Créer un autre compte
plugin-poly-signup-back-btn = ← Retour

# --- Partagé ---
plugin-poly-signup-connecting = Connexion…
plugin-poly-signup-no-identity = Poly n'a pas pu préparer une clé d'identité pour l'inscription.

# --- Page des paramètres du plugin ---
plugin-poly-title = Poly Server
plugin-poly-settings-description = Configurez les options de connexion pour le backend Poly Server.
plugin-poly-setting-websocket-label = Utiliser WebSocket pour les événements en temps réel
plugin-poly-setting-websocket-desc = Lorsqu'il est activé, Poly ouvre une connexion WebSocket persistante pour recevoir les messages et événements instantanément. Désactivez pour revenir à l'interrogation HTTP. Nécessite une reconnexion ou un redémarrage de l'application pour prendre effet.

# --- Onglet profil du compte ---
plugin-poly-profile-title = Profil
plugin-poly-profile-section-desc = Gérez les informations de votre profil Poly Server.
plugin-poly-profile-avatar-label = Photo de profil
plugin-poly-profile-display-name-label = Nom d'affichage
plugin-poly-profile-display-name-desc = Votre nom d'affichage est visible par les autres utilisateurs de ce serveur.
plugin-poly-profile-background-label = Bannière / Arrière-plan
plugin-poly-profile-background-desc = Image de bannière sur votre profil (bientôt disponible).
plugin-poly-profile-status-label = Statut actuel
plugin-poly-profile-status-desc = Votre disponibilité visible par les autres utilisateurs.
plugin-poly-profile-status-online = En ligne
plugin-poly-profile-status-away = Absent
plugin-poly-profile-status-dnd = Ne pas déranger
plugin-poly-profile-status-appear-offline = Apparaître hors ligne
plugin-poly-profile-save = Enregistrer le profil
plugin-poly-profile-saved = Profil enregistré !
plugin-poly-profile-avatar-coming-soon = Téléchargement d'avatar bientôt disponible.
plugin-poly-profile-banner-coming-soon = Téléchargement de bannière bientôt disponible.

# keys added by P46/P47
plugin-poly-menu-invite-people-label = Inviter des personnes

plugin-poly-menu-privacy-settings-label = Paramètres de confidentialité

plugin-poly-menu-edit-per-server-profile-label = Modifier le profil du serveur

plugin-poly-menu-federation-settings-label = Paramètres de fédération

plugin-poly-setting-profile-label = Profil

plugin-poly-setting-nickname-label = Pseudo

plugin-poly-setting-nickname-desc = Afficher ce nom à la place de votre nom de compte sur ce serveur.

plugin-poly-setting-avatar-url-label = URL de l'avatar

plugin-poly-setting-avatar-url-desc = URL de l'image à utiliser comme avatar sur ce serveur. Laissez vide pour utiliser l'avatar de votre compte.

plugin-poly-setting-privacy-label = Confidentialité

plugin-poly-setting-allow-dms-from-server-members-label = Autoriser les DMs des membres

plugin-poly-setting-allow-dms-from-server-members-desc = Lorsqu'activé, les autres membres de ce serveur peuvent vous envoyer des messages directs.

plugin-poly-setting-federation-label = Fédération

plugin-poly-setting-allow-federation-label = Autoriser la fédération

plugin-poly-setting-allow-federation-desc = Lorsqu'activé, ce serveur peut communiquer avec d'autres serveurs Poly fédérés.
