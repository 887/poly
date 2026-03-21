# Poly — Español (es) traducciones principales
# Project Fluent (.ftl) format

# Aplicación
app-title = Poly
electron-window-minimize = Minimizar
electron-window-maximize = Maximizar o restaurar
electron-window-close = Cerrar ventana
app-description = Cliente de mensajería multiplataforma
wasm-crash-title = Poly sufrió un fallo del navegador
wasm-crash-description = La página actual falló o lanzó un error de navegador/WASM no controlado. La UI debajo de esta superposición ya no es confiable.
wasm-crash-details-label = Tipo de fallo
wasm-crash-location-label = Ubicación del código
wasm-crash-path-label = Ruta
wasm-crash-reload-action = Recargar Poly
wasm-crash-kind-panic = Panic de Rust
wasm-crash-kind-window-error = Evento de error del navegador
wasm-crash-kind-unhandled-rejection = Rechazo de promesa no controlado
wasm-crash-kind-unknown = Fallo desconocido
wasm-crash-generic-message = El navegador no proporcionó detalles del fallo.
wasm-crash-window-error-fallback = El navegador informó un evento de error global sin mensaje.
wasm-crash-rejection-fallback = Se rechazó una promesa sin un mensaje de error legible.

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
setup-welcome-tagline = Un cliente de mensajería multi-cuenta impulsado por plugins. Conecta todas tus plataformas de chat en un solo lugar.
setup-feature-plugins = Basado en plugins — añade soporte para cualquier mensajero mediante plugins WASM
setup-feature-multi-account = Multi-cuenta — gestiona todas tus cuentas en todas las plataformas
setup-feature-demo = Datos de demostración cargados — explora la app con conversaciones de ejemplo
setup-feature-keys = Claves de identidad — genéralas en Ajustes → Identidad cuando estés listo
setup-get-started = Comenzar
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

# Usuarios / Estado
user-online = En línea
user-idle = Ausente
user-dnd = No molestar
user-invisible = Invisible
user-offline = Desconectado
user-away = Ausente
user-appear-offline = Aparecer desconectado
user-members = Miembros
user-no-members = Sin miembros

# Barra de cuenta — insignias en esquinas del avatar
account-profile-click-hint = Haz clic para ver tu perfil
account-conn-connected = Conectado
account-conn-connecting = Conectando…
account-conn-disconnected = Sin conexión
account-conn-error = Error de conexión

# Selector de estado emergente
status-picker-title = Establecer estado

# Filtro de lista de miembros
member-filter-placeholder = Buscar miembros…
member-filter-tooltip = Buscar miembros
member-filter-no-results = Ningún miembro coincide con esa búsqueda.

# User profile modal
user-profile-more-options = Más opciones
user-profile-message = Mensaje
user-profile-call = Llamada
user-profile-video = Vídeo
user-profile-add-to-call = Añadir a la llamada
user-profile-add-video-to-call = Añadir video a la llamada
user-profile-note = Nota
user-profile-note-placeholder = Haz clic para añadir una nota
user-profile-open = Ver perfil

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
settings-no-accounts = No hay cuentas conectadas. Añade una cuenta para empezar.
settings-account-settings-link = Configuración de cuenta
account-switch = Cambiar cuenta
account-settings = Configuración de cuenta
settings-account-settings = Configuración de cuenta

# Flujo de registro — selección de backend
signup-picker-title = Añadir cuenta
signup-picker-description = Elige qué tipo de cuenta añadir.
signup-picker-back = ← Volver a ajustes
signup-stub-back = ← Elegir backend
# ── Crear servidor / canal ───────────────────────────────────────────────────
create-server-btn = Crear servidor
create-server-placeholder = Nombre del servidor…
create-server-submit = Crear
create-server-cancel = Cancelar
create-server-creating = Creando…
create-server-page-title = Crear un servidor
create-server-page-subtitle = Dale un nombre a tu servidor. Siempre puedes cambiarlo después.
create-server-page-label = Nombre del servidor
channel-list-text-channels = Canales de texto
create-channel-btn = Nuevo canal
create-channel-page-title = Crear un canal
create-channel-page-subtitle = Dale un nombre a tu canal. Siempre puedes cambiarlo después.
create-channel-page-label = Nombre del canal
create-channel-placeholder = Nombre del canal…
create-channel-submit = Crear
create-channel-cancel = Cancelar
create-channel-creating = Creando…
settings-backup = Servidores de respaldo
settings-backup-description = Configurar servidores de sincronización cifrada
settings-add-backup = Agregar servidor de respaldo
settings-identity = Identidad
settings-identity-description = Tu identidad del dispositivo, frase de recuperación y dónde se usa esta identidad
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
settings-layout = Diseño
settings-layout-description = Comportamiento de diseño y reflejo entre las interfaces de escritorio y móvil
settings-general = General
settings-general-description = Restablece los datos locales de la app o destruye por completo el estado para pruebas limpias
settings-layout-mode = Modo de diseño
settings-layout-mode-description = Elige si Poly debe detectar móvil por ancho, por orientación vertical, o forzar siempre escritorio/móvil. Los overrides por URL como ?layout=mobile o ?layout=desktop tienen prioridad mientras estén presentes.
settings-layout-auto-width = Auto (ancho ≤ 640px)
settings-layout-auto-portrait = Auto (vertical)
settings-layout-force-desktop = Forzar escritorio
settings-layout-force-mobile = Forzar móvil
settings-mirror-menu-layout = Reflejar menús / alas de la app
settings-mirror-menu-layout-description = Intercambia las alas izquierda y derecha de la app en escritorio y móvil, incluyendo el orden de las barras laterales y los botones del encabezado móvil.
settings-mirror-chat-messages = Reflejar mensajes del chat
settings-mirror-chat-messages-description = Coloca los avatares / canaletas del mensaje a la derecha manteniendo el texto legible.
settings-force-mobile-layout = Forzar diseño móvil
settings-force-mobile-layout-description = Usa la interfaz móvil incluso por encima de 640px. Déjalo desactivado para usar la interfaz de escritorio hasta que la ventana sea naturalmente estrecha.
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

# Gestor de plugins
settings-plugins = Plugins
settings-plugins-description = Activar o desactivar los plugins de backend de mensajería. Cada plugin es un cliente de mensajería. Las cuentas son sesiones creadas por esos plugins.
plugins-native-title = Plugins integrados
plugins-native-description = Estos plugins están compilados en esta versión de Poly. Actívalos o desactívalos con las casillas de verificación.
plugins-loaded-count = Backends activos
plugins-none-loaded = No hay plugins WASM añadidos aún. Añade una URL de plugin abajo.
plugins-status-disconnected = Desconectado
plugins-status-connecting = Conectando…
plugins-status-connected = Conectado
plugins-status-error = Error
plugins-type-native = Nativo
plugins-type-wasm = WASM
plugins-not-compiled = no en este build
plugins-active-accounts = Cuentas activas
plugins-wasm-title = Plugins WASM
plugins-wasm-description = Los plugins WASM extienden Poly con backends adicionales. Carga un plugin desde una URL — Poly añadirá automáticamente la versión WIT.
plugins-add-wasm-title = Añadir plugin desde URL
plugins-add-wasm-description = Introduce la URL base de un plugin WASM. La versión WIT se añadirá automáticamente.
plugins-url-placeholder = https://plugins.example.com/matrix.wasm
plugins-name-placeholder = Nombre a mostrar (opcional)
plugins-add-btn = Añadir plugin
plugins-url-required = Por favor introduce una URL de plugin
plugins-remove = Eliminar
plugins-wit-hint = Versión de interfaz WIT

# Ajustes de plugins
settings-plugin-settings = Ajustes de plugins
# Etiqueta que aparece antes de las secciones de plugins en el menú de ajustes
settings-plugins-section-divider = Ajustes de plugins
# Encabezado de grupo en la barra lateral que separa las secciones integradas de las páginas de plugins
settings-plugin-settings-nav-header = Ajustes de plugins
# Pequeña insignia para las secciones proporcionadas por plugins
# Pequeña insignia para secciones de plugins
settings-plugins-badge = Plugin
plugin-settings-nav-title = Ajustes de backends
plugin-settings-none = No hay backends con ajustes cargados. Activa los datos demo o conecta una cuenta.
plugin-settings-generic-description = Este backend aún no tiene ajustes personalizados. Los ajustes aparecerán aquí cuando el plugin los soporte.
# Nota: las cadenas plugin-demo-* se cargan desde el bundle FTL del plugin de demostración.

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
settings-identity-create-btn = Crear identidad
settings-identity-creating = Creando…
settings-identity-purpose = Este material de identidad es la clave que Poly usa en tu nombre:
settings-identity-purpose-poly = Los servidores Poly la usan para inicio de sesión con clave y funciones cifradas de extremo a extremo.
settings-identity-purpose-backup = Los servidores de copia de seguridad la usan para derivar claves de cifrado y autenticar la sincronización cifrada.
settings-identity-backup-servers = Servidores de copia de seguridad
settings-identity-backup-servers-description = Esta identidad se utiliza para la autenticación en los siguientes servidores de copia de seguridad.
settings-identity-poly-accounts = Cuentas de Poly Server
settings-identity-poly-accounts-description = Esta identidad se utiliza para las siguientes cuentas en servidores Poly auto-hospedados.
settings-identity-no-servers = Ningún servidor de copia de seguridad configurado aún.
settings-identity-no-poly-accounts = Sin cuentas de Poly server.
settings-identity-status-active = Activo
settings-identity-status-disabled = Desactivado
settings-identity-delete = Eliminar identidad
settings-identity-delete-confirm-title = ¿Eliminar identidad?
settings-identity-delete-confirm-message = Esto eliminará permanentemente esta clave de identidad. ¡Asegúrate de tener la frase de recuperación respaldada o no podrás recuperar el acceso!
settings-identity-delete-confirm = Sí, eliminar
settings-identity-cancel = Cancelar

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
action-more = Mas
chat-replying-to = Respondiendo a { $name }
action-search = Buscar
action-copy = Copiar
action-back = Atrás
action-confirm = Confirmar
action-clear = Borrar
action-download = Descargar
action-open-in-browser = Abrir en el navegador
zoom-in = Acercar
zoom-out = Alejar

media-viewer-unavailable-title = Medio no disponible
media-viewer-unavailable-body = No se pudo cargar este medio desde el estado actual del chat.

# Errores
error-generic = Algo salió mal. Por favor, inténtalo de nuevo.
error-network = Error de red. Verifica tu conexión.
error-auth-failed = Error de autenticación. Verifica tus credenciales.
error-not-found = No encontrado.

# Voz / Video
voice-connected = Voz conectada
voice-join-voice = Unirse a voz
voice-join-video = Unirse a video
voice-direct-call = Llamada directa
voice-group-call = Llamada grupal
voice-swap-held-call = Cambiar a llamada en espera
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
voice-in-call = en la llamada
voice-go-to-channel = Ir al canal
voice-go-to-conversation = Ir a la conversación
direct-call-calling = Llamando…
direct-call-calling-video = Iniciando videollamada…
direct-call-adding = Añadiendo a la llamada…
direct-call-adding-video = Añadiendo video a la llamada…
direct-call-awaiting-join = Esperando a que la llamada conecte
direct-call-ringing = Sonando… toca × para cancelar
direct-call-cancel = Cancelar llamada
voice-mute-mic = Silenciar micrófono
voice-unmute-mic = Activar micrófono
voice-camera = Activar cámara
mobile-nav-open = Abrir menú de navegación
mobile-nav-close = Cerrar menú de navegación
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
settings-search-no-results = No se encontraron ajustes para esta búsqueda.
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
dm-saved-messages = Mensajes guardados
dm-new-conversation = Nueva conversación
dm-search-conversations = Buscar conversaciones
dm-search-placeholder = Encuentra o inicia una conversación
saved-items-title = Mensajes guardados
saved-items-description = Vuelve a los mensajes fijados de tus mensajes directos y grupos.
saved-items-empty = Aún no hay mensajes fijados.
saved-items-all-sources = Todas las fuentes
saved-items-filter-placeholder = Filtrar fuentes guardadas...
saved-items-sources-empty = No se encontraron fuentes guardadas
dm-no-results = No se encontraron conversaciones

# Friends panel
friends-title = Amigos
friends-management-title = Personas
friends-management-description = Administra amigos, usuarios ignorados y usuarios bloqueados para esta cuenta.
friends-management-message = Enviar mensaje
friends-ignored-title = Ignorados
friends-ignored-empty = Todavía no hay usuarios ignorados.
new-conversation-description = Elige un amigo para iniciar una conversación directa. Las conversaciones con varias personas usarán este compositor cuando la creación de grupos compartida esté conectada.
new-conversation-start-dm = Iniciar conversación
new-conversation-group-pending = Las conversaciones con varias personas llegarán enseguida.
conversation-search-title = Buscar conversaciones
conversation-search-description = Busca en los mensajes directos y grupos de { $account }.
friends-search-placeholder = Buscar amigos...
friends-none = No se encontraron amigos
notifications-filter-all-types = Todas las notificaciones
notifications-filter-mentions = Menciones
notifications-filter-friend-requests = Solicitudes de amistad
notifications-filter-server-invites = Invitaciones al servidor
notifications-filter-voice-invites = Invitaciones de voz
notifications-filter-other = Otras
notifications-unread-count = sin leer
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
user-all-offline-hidden = Todos los miembros están desconectados y ocultos
account-not-signed-in = No conectado

# Configuración del chat — lista de miembros
chat-settings-member-list = Lista de miembros
chat-settings-grouping = Agrupación
chat-settings-grouping-by-status = Por estado
chat-settings-grouping-none = Sin agrupación
chat-settings-sort-order = Orden
chat-settings-sort-alphabetical = Alfabético
chat-settings-sort-online-first = Conectados primero
chat-settings-sort-join-order = Orden de entrada
chat-settings-show-offline = Mostrar miembros desconectados

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

# Search page
search-page-title = Búsqueda
search-page-placeholder = Buscar servidores, canales, DMs, grupos…
search-page-accounts = Cuentas
search-page-dms = Mensajes Directos
search-page-groups = Grupos
search-page-type-filter = Mostrar
search-type-servers = Servidores
search-type-dms = DMs
search-type-groups = Grupos
search-showing-of = Mostrando { $count } de { $total }
search-load-more = Desplaza para ver más…
