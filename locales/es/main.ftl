# Poly — Español (es) traducciones principales
# Project Fluent (.ftl) format

# Aplicación
app-title = Poly
electron-window-minimize = Minimizar
electron-window-maximize = Maximizar o restaurar
electron-window-close = Cerrar ventana
app-description = Cliente de mensajería multiplataforma

# Navegación
nav-dms = Mensajes directos
nav-friends = Amigos
nav-notifications = Notificaciones
nav-settings = Configuración
nav-search = Buscar
nav-servers = Servidores
nav-demo = Alternar cliente de demostración
nav-demo-active = Cliente de demostración activo

# Asistente de configuración
setup-welcome-title = Bienvenido a Poly
setup-welcome-description = Un mensajero unificado para todas tus plataformas de chat.
setup-generating-keys = Generando tus claves de identidad...
setup-your-account-id = Tu ID de cuenta
setup-account-id-description = Este es tu identificador único. Compártelo con amigos para conectarte.
setup-recovery-phrase = Frase de recuperación
setup-recovery-phrase-description = Escribe estas palabras y guárdalas en un lugar seguro. Las necesitarás para recuperar tu cuenta.
setup-recovery-warning = Si pierdes tu frase de recuperación, perderás permanentemente el acceso a tu cuenta.
setup-copy-phrase = Copiar frase
setup-export-phrase = Exportar a archivo
setup-confirm-phrase = Confirmar frase de recuperación
setup-confirm-description = Ingresa las palabras de tu frase de recuperación para confirmar que las has guardado.
setup-continue = Continuar
setup-skip-confirmation = Omitir confirmación
setup-complete = Configuración completa
setup-complete-description = Tu identidad ha sido creada. Agrega cuentas de mensajería en Configuración.
setup-go-to-app = Ir a Poly

# Chat
chat-type-message = Escribe un mensaje...
chat-send = Enviar
chat-typing = { $user } está escribiendo...
chat-typing-multiple = { $count } personas están escribiendo...
chat-no-messages = Aún no hay mensajes. ¡Inicia la conversación!
chat-load-more = Cargar más
chat-edited = (editado)
chat-loading = Cargando mensajes...
chat-select-conversation = Selecciona una conversación
chat-loading-earlier = Cargando mensajes anteriores...
chat-unread-banner = { $count } mensajes nuevos desde las { $time } del { $date }
chat-unread-divider = Nuevo

# Canales
channel-text = Canal de texto
channel-voice = Canal de voz
channel-video = Canal de video

# Usuarios
user-online = En línea
user-idle = Ausente
user-dnd = No molestar
user-invisible = Invisible
user-offline = Desconectado
user-members = Miembros

# Notificaciones
notifications-title = Notificaciones
notifications-empty = No hay nuevas notificaciones
notifications-mark-read = Marcar como leído
notifications-dismiss = Descartar
notifications-mention = { $user } te mencionó en { $channel }
notifications-friend-request = { $user } te envió una solicitud de amistad
notifications-server-invite = Has sido invitado a { $server }

# Configuración
settings-title = Configuración
settings-accounts = Cuentas
settings-accounts-description = Administra tus cuentas de mensajería
settings-add-account = Agregar cuenta
settings-remove-account = Eliminar cuenta
account-switch = Cambiar cuenta
account-settings = Configuración de cuenta
settings-account-settings = Configuración de cuenta
settings-backup = Servidores de respaldo
settings-backup-description = Configurar servidores de sincronización cifrada
settings-add-backup = Agregar servidor de respaldo
settings-identity = Identidad
settings-identity-description = Tu identidad de Poly y opciones de recuperación
settings-your-id = Tu ID de cuenta
settings-export-recovery = Exportar frase de recuperación
settings-theme = Tema
settings-theme-description = Personalizar colores, temas y apariencia
settings-media = Multimedia
settings-media-description = Configura proveedores de GIF y futuras integraciones multimedia
settings-media-active-provider = Proveedor GIF activo
settings-media-api-key = Clave API
settings-media-api-key-placeholder = Pega la clave API del proveedor
settings-media-provider-klippy = Klippy
settings-media-provider-giphy = Giphy
settings-media-provider-imgur = Imgur
settings-media-status-configured = Configurado
settings-media-status-not-setup = Sin configurar
settings-theme-preset = Preset de tema
settings-theme-custom-css = CSS personalizado
settings-theme-import = Importar tema
settings-theme-export = Exportar tema
settings-color-mode = Modo de color
settings-color-overrides = Personalización de colores
settings-color-hint = Activa esta opción para reemplazar colores individuales del tema. Desactívala para volver al tema predeterminado.
settings-reset-colors = Restablecer colores
settings-theme-apply-css = Aplicar CSS
settings-css-hint = Descomenta una variable para reemplazar el tema. El interruptor activa/desactiva estas modificaciones CSS.
settings-css-reset-template = Restablecer plantilla
settings-language = Idioma
settings-language-description = Elige tu idioma preferido
settings-appearance = Apariencia
settings-appearance-description = Modo oscuro, modo claro y opciones de visualización
settings-dark-mode = Modo oscuro
settings-light-mode = Modo claro
settings-follow-device = Seguir preferencia del dispositivo
settings-general = General
settings-general-description = Preferencias de notificación y comportamiento de inicio
settings-reset-description = Restablece los datos de la app para empezar de nuevo, o destruye todo el estado local para pruebas limpias.
settings-reset-app = Restablecer datos de la app
settings-nuke-app = NUKear estado de la app
settings-reset-error-no-storage = El almacenamiento aún no está listo
settings-reset-error-failed = Error al restablecer los datos de la app
settings-nuke-error-failed = Error al NUKear el estado de la app
settings-reset-error-reload = Restablecimiento exitoso, pero falló la recarga

# Configuración de Demo
settings-demo = Demo
settings-demo-description = Administra el cliente de datos demo integrado. Cuando está habilitado, Poly carga cuentas de ejemplo con servidores, canales y conversaciones para explorar la aplicación.
settings-demo-toggle = Activar datos demo

# Backup Server Settings
settings-backup-add-server = Agregar servidor
settings-backup-url-placeholder = http://127.0.0.1:8080
settings-backup-url-label = URL del servidor
settings-backup-label-label = Nombre del servidor
settings-backup-passphrase-label = Frase de contraseña del servidor
settings-backup-connect = Conectar
settings-backup-connecting = Conectando...
settings-backup-cancel = Cancelar
settings-backup-status-unknown = Desconocido
settings-backup-status-connected = Conectado
settings-backup-status-auth-required = Autenticación requerida
settings-backup-status-unreachable = Inaccesible
settings-backup-status-syncing = Sincronizando...
settings-backup-sync-now = Sincronizar ahora
settings-backup-reauth = Volver a autenticar
settings-backup-remove = Eliminar
settings-backup-last-synced = Última sync: { $time }
settings-backup-never-synced = Nunca sincronizado
settings-backup-enabled = Habilitado
settings-backup-auth-success = ¡Conectado!
settings-backup-auth-failed = Falló la autenticación
settings-backup-no-servers = No hay servidores de respaldo configurados.
settings-backup-wizard-step1 = URL del servidor
settings-backup-wizard-step2 = Conectar
settings-backup-step1-hint = Introduce la URL de tu servidor de respaldo Poly
settings-backup-step2-hint = Pon un nombre e introduce las credenciales para terminar
settings-backup-check-btn = Comprobar conexión
settings-backup-checking = Comprobando…
settings-backup-continue = Continuar
settings-backup-back = Atrás
settings-backup-finish = Finalizar configuración
settings-backup-url-empty = Por favor, introduce una URL de servidor
settings-backup-password-required = 🔒 Contraseña requerida
settings-backup-no-password-required = ✓ Sin contraseña requerida
settings-backup-server-full = Servidor lleno — registros desactivados

# Identity Settings
settings-identity-your-id-label = Tu ID de cuenta Poly
settings-identity-copy-id = Copiar ID
settings-identity-show-phrase = Mostrar frase de recuperación
settings-identity-phrase-modal-title = Tu frase de recuperación
settings-identity-phrase-warning = Mantén esta frase en secreto. Quien la tenga puede acceder a tu cuenta.
settings-identity-copy-all = Copiar todas las palabras
settings-identity-close = Cerrar
settings-identity-no-identity = Identidad no generada aún. Completa primero el asistente de configuración.

# Presets de tema
theme-blue = Azul
theme-purple = Púrpura
theme-red = Rojo
theme-green = Verde
theme-monotone = Monotono

# Backends
backend-stoat = Stoat
backend-matrix = Matrix
backend-discord = Discord
backend-teams = Teams
backend-demo = Demo

# Acciones comunes
action-save = Guardar
action-cancel = Cancelar
action-delete = Eliminar
action-edit = Editar
action-close = Cerrar
chat-replying-to = Respondiendo a { $name }
action-search = Buscar
action-copy = Copiar
action-back = Atrás
action-confirm = Confirmar

# Errores
error-generic = Algo salió mal. Por favor, inténtalo de nuevo.
error-network = Error de red. Verifica tu conexión.
error-auth-failed = Error de autenticación. Verifica tus credenciales.
error-not-found = No encontrado.

# Voz / Video
voice-connected = Voz conectada
voice-join-voice = Unirse a voz
voice-join-video = Unirse a video
voice-disconnect = Desconectar
voice-muted = Silenciado
voice-deafened = Ensordecido
voice-streaming = Compartiendo pantalla
voice-video-on = Cámara encendida
voice-mute = Silenciar
voice-unmute = Activar micrófono
voice-deafen = Ensordecer
voice-undeafen = Activar audio
voice-no-channel = Ningún canal seleccionado
voice-no-one-here = Nadie está aquí todavía
voice-be-first = ¡Sé el primero en unirte!
voice-watching-screen = Viendo pantalla compartida
voice-in-channel = en el canal
voice-go-to-channel = Ir al canal
voice-mute-mic = Silenciar micrófono
voice-unmute-mic = Activar micrófono
voice-camera = Activar cámara
voice-screen-share = Compartir pantalla
voice-activity = Compartir actividad
voice-voiceboard = Tablero de voz
voice-signal-quality = Calidad de señal
voice-stop-camera = Parar cámara
voice-stop-share = Parar compartir
voice-camera-preview = Vista previa cámara
voice-screen-sharing = Vista previa pantalla compartida
voice-audio-settings = Configuración de voz y audio
voice-mic-device = Dispositivo de entrada (Micrófono)
voice-speaker-device = Dispositivo de salida (Altavoz)
voice-default-device = Predeterminado
voice-noise-cancel = Cancelación de ruido
voice-noise-cancel-desc = Elimina el ruido de fondo con reducción IA (RNNoise).
voice-noise-cancel-on = Cancelación de ruido: Activada
voice-noise-cancel-off = Cancelación de ruido: Desactivada
voice-server-location = Ubicación del servidor
voice-testing-mic = Probando... (3s)
voice-test-mic = Probar micrófono (3 seg)

# Emoji / GIF / Reacciones
emoji-picker = Emoji
emoji-search = Buscar emoji...
gif-picker = GIF
reaction-add = Añadir reacción

# Barra de acciones de mensaje / menú contextual
msg-reply = Responder
msg-forward = Reenviar
msg-edit = Editar
msg-delete = Eliminar
msg-copy-text = Copiar texto
msg-apps = Aplicaciones
msg-mark-unread = Marcar como no leído
msg-copy-link = Copiar enlace del mensaje
msg-speak = Leer mensaje en voz alta
msg-report = Reportar mensaje
msg-copy-id = Copiar ID del mensaje
msg-edit-save = Guardar
msg-edit-cancel = Cancelar

chat-drop-files = Suelta archivos para subir
chat-attach-file = Adjuntar archivo

# Navegación
nav-back = Atrás
nav-forward = Adelante

# Settings search
settings-search = Buscar ajustes...
settings-voice-video = Voz y Video
settings-notifications = Notificaciones
account-settings-title = Configuración de cuenta

# Voice & Video settings
voice-input-device = Dispositivo de entrada
voice-output-device = Dispositivo de salida
voice-input-volume = Volumen de entrada
voice-output-volume = Volumen de salida
voice-mic-test = Probar micrófono
voice-mic-test-stop = Detener prueba
voice-input-mode = Modo de entrada
voice-input-vad = Detección de actividad de voz
voice-input-ptt = Pulsar para hablar
voice-noise-suppression = Supresión de ruido
voice-noise-off = Desactivado
voice-noise-standard = Estándar
voice-noise-high = Alto
voice-echo-cancel = Cancelación de eco

# Notifications settings
notif-enable-desktop = Activar notificaciones de escritorio
notif-permission-request = Permitir notificaciones
notif-global-header = Global (Dispositivo)
notif-notify-about = Notificarme sobre
notif-sounds = Sonidos
notif-badges = Insignias
notif-streams = Personas que conozco empiezan a transmitir
notif-friends-voice = Amigos se unen a canales de voz
notif-reactions = Alguien reacciona a mis mensajes
notif-sounds-new-message = Mensaje nuevo
notif-sounds-dm = Mensajes directos
notif-sounds-ring = Llamada entrante
notif-badge-unread = Activar insignia de mensajes no leídos
notif-no-accounts = Sin cuentas activas. Agrega una cuenta en Ajustes → Cuentas.

# DM list
dm-search-placeholder = Encuentra o inicia una conversación
dm-no-results = No se encontraron conversaciones

# Friends panel
friends-title = Amigos
friends-search-placeholder = Buscar amigos...
friends-none = No se encontraron amigos
filter-all = Todas las cuentas
filter-all-servers = Todos los servidores

# Formateo de tiempo
time-just-now = ahora mismo
time-one-minute-ago = hace 1 minuto
time-minutes-ago = hace { $count } minutos
time-one-hour-ago = hace 1 hora
time-hours-ago = hace { $count } horas
time-one-day-ago = hace 1 día
time-days-ago = hace { $count } días

# Chat extras
chat-toggle-members = Mostrar/ocultar lista de miembros
chat-toggle-contact = Mostrar/ocultar información del contacto
chat-select-channel = Selecciona un canal para empezar a chatear
chat-timestamp-yesterday = Ayer { $time }
search-messages = Buscar mensajes
search-placeholder = Buscar en este canal...
search-placeholder-channel = Buscar en #{ $channel }
search-placeholder-user = Buscar { $user }
search-placeholder-group = Buscar { $group }
search-results = Resultados
search-no-results = Ningún mensaje coincide con esa búsqueda
search-filter-from-user = De una persona específica
search-filter-from-user-subtitle = de: usuario
search-filter-in-channel = Enviado en un canal específico
search-filter-in-channel-subtitle = en: canal
search-filter-has-link = Incluye un tipo de dato específico
search-filter-has-link-subtitle = tiene: enlace, incrustación o archivo
search-filter-mentions = Menciona a una persona específica
search-filter-mentions-subtitle = menciones: usuario
search-filter-more = Más filtros
search-filter-more-subtitle = fechas, tipo de autor y más
pinned-messages = Mensajes fijados
no-pinned-messages = No hay mensajes fijados
threads = Hilos
no-threads = Todavía no hay hilos
chat-notifications = Notificaciones
chat-no-notifications = No hay notificaciones aquí
chat-type-message-channel = Escribe en #{ $channel }
chat-type-message-user = Escribe a { $user }
chat-type-message-group = Escribe en { $group }
chat-markdown-formatting = Formato Markdown

# Usuarios extras
user-no-members = No hay miembros que mostrar
account-not-signed-in = No conectado

# Etiquetas de color
color-accent = Acento
color-background = Fondo
color-surface = Superficie
color-text = Texto
color-secondary-text = Texto secundario
color-border = Borde
color-favorites-bar = Fondo barra de favoritos
color-account-bar = Fondo barra de cuentas

# Dispositivos de audio predeterminados
voice-default-mic = Micrófono predeterminado
voice-default-speakers = Altavoces predeterminados

# Mensajes de error
error-storage-unavailable = Almacenamiento no disponible
error-load-settings = Error al cargar la configuración
error-reload-servers = Error al recargar los servidores

# Server context menu
server-menu-mark-read = Marcar como leído
server-menu-invite = Invitar al servidor
server-menu-unmute = Reactivar sonido del servidor
server-menu-mute = Silenciar servidor
server-menu-notif-settings = Configuración de notificaciones
server-menu-hide-muted = Ocultar canales silenciados
server-menu-show-all = Mostrar todos los canales
server-menu-privacy = Configuración de privacidad
server-menu-edit-profile = Editar perfil del servidor
server-menu-leave = Salir del servidor
server-menu-copy-id = Copiar ID del servidor
server-menu-add-favorites = Agregar a favoritos
server-menu-remove-favorites = Eliminar de favoritos

# Remove from favorites inline confirm
remove-favorites-title = ¿Eliminar «{ $name }» de favoritos?
remove-favorites-body = Puedes volver a agregarlo en cualquier momento arrastrándolo a la barra de favoritos o usando este menú.
remove-favorites-cancel = Cancelar
remove-favorites-confirm = Eliminar
# Menú desplegable del banner del servidor
server-banner-settings = Configuración del servidor
server-banner-invite = Invitar personas
server-banner-notif-settings = Configuración de notificaciones
server-banner-create-channel = Crear canal
server-banner-channels-roles = Canales y roles
server-banner-browse-channels = Explora canales y activa las categorías de este servidor.
server-banner-channel-count = canales
server-banner-leave = Salir del servidor

# Server settings page
# Configuración del servidor
server-settings-title = Configuración del servidor
server-settings-overview = Descripción general
server-settings-notifications = Notificaciones
server-settings-profile = Perfil
server-settings-general = General

# Descripción general del servidor (icono + banner)
server-overview-icon = Icono del servidor
server-overview-icon-url = URL del icono
server-overview-icon-hint = URL de la imagen del icono. Se recomienda SVG o PNG en formato cuadrado.
server-overview-banner = Banner del servidor
server-overview-banner-url = URL del banner
server-overview-banner-hint = URL de la imagen del banner ancho que aparece sobre la lista de canales. Se recomienda formato horizontal (p. ej. 960×240).
server-overview-save = Guardar
server-overview-saved = Guardado
server-overview-local-override = Reemplazar icono localmente
server-overview-local-override-hint = Este backend no admite iconos de servidor personalizados. El icono definido aquí se almacena solo en este dispositivo.

# Leave server inline confirm
leave-server-title = ¿Salir de «{ $name }»?
leave-server-body = No podrás volver a unirte a menos que seas reinvitado.
leave-server-cancel = Cancelar
leave-server-confirm = Salir del servidor

# Server notification settings
server-notif-all = Todos los mensajes
server-notif-mentions = Solo @menciones
server-notif-nothing = Nada
server-notif-suppress-everyone = Suprimir @everyone y @here
server-notif-suppress-roles = Suprimir todas las @menciones de rol
server-notif-suppress-highlights = Suprimir destacados
server-notif-mute-events = Silenciar nuevos eventos
server-notif-mobile-push = Notificaciones push móvil

# Server profile settings
server-profile-nickname = Apodo en el servidor
server-profile-nickname-hint = Cambia cómo apareces en este servidor
server-profile-save = Guardar cambios

# Server general settings
server-general-info = Información del servidor
server-general-danger = Zona de peligro

# Group DMs
group-members-title = Miembros
group-member-remove = Eliminar
group-member-remove-tooltip = Eliminar a { $name } de este grupo

# DM header
dm-header-subtitle = Mensaje directo

# Presence status labels
presence-online = En línea
presence-away = Ausente
presence-dnd = No molestar
presence-offline = Desconectado

# DM contact panel
dm-contact-panel-title = Info del contacto
dm-contact-not-found = Contacto no encontrado

# Demo backend
demo-regenerate-data = Regenerar datos de demostración
