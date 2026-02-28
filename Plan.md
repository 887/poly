# Plan

So i need you to build me a cross platform multi client backend chat client in rust + Dioxus.
For storing settings cross client i want to be able to use surealdb.

Let's call this App "Poly" for PolyGlot Messenger for now since poly is nice and short.

i want you to look at this first and maybe clone it
https://github.com/verystochastic/dioxus-surrealdb-template
I want to use the latest dioxus version 0.7.3 and write yourself an agent's md to always use up to date rust stable and dioxus 7.3 features.
https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.3
Also same for surreal db use version 3.0 and have in the agents.md to ONLY use 3.0 documentation. Surrealdb can use a rocksdb backend for now
https://surrealdb.com/3.0



The plan is to have a singular monorepo codebase for:

- Desktop
  - Windows
  - Linux
  - Mac
- Android
- iOS
- Web (their web version that uses axum)
- Surrealdb account syncing server (call this BackupServer)

For the Desktop apps i want a build that creates:

- the Dioxus-desktop version
- the Dioous-desktop version with the blitz renderer
- the Dioous-desktop version but build with an electron app wrapper

- a single shared functionality library crate for everything that is the main component i'll be working with.

The library crate crate is what needs to support dioxus rust Subsecond hot reloading, since it's the main component we'll code our shared logic in.
You failed the entire plan if we can't get hot reloading to work in that library.

Dioxus should imply the tokio rust runtime in multithreaded, if this wasn't clear.

For all of the different configurations want:

- in the .vscode folder:
  - a launch.json that can start each version (this is linux not macos, but create a mac startupable version for when i check this out on macos still)
  - runner tasks that compile the exe/binaries/apk what ever end format
- github action runners that build the entire codebase cascadingly but also a dumb 'just compile the library' action that compiles just that.

All of this is pure project structure.
Since this is a new project and a monorepo we probably need a whole bunch of .gitignore files for each and every individual crate/app.
The .vscode folder should be on the main repo though and we probably will have the library and 6 crates for Windows/Linux/Mac/Android/Ios/Web 
The account syncing server has only the purpose to synchronize the settings from the local surrealdb so i can make a cloud backup. 


All the account information in the synchornized settings needs to be encrypted before sending it to a backup server. 
We do not want to store anything on that server that isn't encrypted by the user in our shared backend database, so the user needs on initial setup to setup their own encryption key and save the recovery information somewhere or back up their memoic phrase. we get their public key on the server, which is used to identify their settings in the db and their 'username'. there is no account logic whats so ever on the server, it's basically a firebase that evolves with storing 'stuff' for the user that he all encrypted localy.
We don't know anything about our users. We do want so a password for our syncinc server, so you can only sync to it when you have that and also a limit for the number of accounts.
When a new user wants to synchronize data to that server after the limit has been reached and the server didn't see their public-key-user-id before (like sessions messenger user ids) it will not let a new account be created and refuse them. same when the server has password and they can't answer the password. this server should probably have a webinterface aswell as a restapi and also use dioxus + axum but also have a rest endpoint for all that syncing, unless surealdb has something better. however for checking for the limit/password we probably want a rest endpoint.
this server is JUST saving our local appsettings from the surealdb in encrypted form. 
in the app i want to be able to add multiple of such storage servers to have encrypted backups on multiple such server.

Next:
So the main library app should be able to load multiple messenger backends and store multiple accounts for that.
Initially i want to support the following messenger backends:
- Matrix
- Stoat/Revolt messenger (both selfhosted and official server)
- Discord
- Microsoft Teams

Also MULTIPLE ACCOUNTS FOR EACH FOR THOSE.

On the main page of the App i want my favourited Servers and a view simillar to discord with servers on the left, then channels, then the messages on the right.

However the server list get's populated by me and is not all the servers i'm part of on discord/stoat/matrix but my 'favourites'.

For that favorites i want to be able to add a "Stoat" Server from a logged in stoat account to the favourites, see it's notification badge and see what account it's from and that it's a stoat server.
When clicking on that server i want to then view that specific server with all it's channels and voice chats, simillar to how you would see it in stoat. 
Same idea for discord.
For Matrix, same if it does have servers, otherwise we just want to add matrix channels to our favourites under one or more self-created matrix-server categories so we can emulate a discord server. If matrix does have servers now use those.
The first In the Favourites list on the left should lead to my direct messages from all accounts and networks and a search function through all my open friend chats from all accounts.

For the "Servers" all of that i need to show the Source discord/matrix/stoat, the Account Icon the server is from  and obviously The server Icon. 
I want a simillar view like discord/stoat/matrix with my icon in the top left, then my direct messages and then a server icon list below, with a tiny icon in the top left of each big server icon for the source network + account it's from. THe source should also be shown in the channel list banner when clicking on a server.

On the right should always be the chat, as close as we can get it to stoat/matrix/discord with displaying pictures as well as the text input at the bottom.
On mobile we probably will have 3 pages, where sliding left provides us the server browser and channel view for the current server and sliding right provides us the list of users in the current channel/call. oh yeah on the far right even on desktop have the typical discord/stoat/matrix list of current users in the channel.

Also I want a settings page where i can add the accounts, let's put that somewhere in the bar where our current chat is displayed in the top right, together with a search on desktop for messages in the current channel.
In the setting page/view i need to be able to see all my accounts from discord/stoat/matrix/teams. Remember i might have multiple matrix/stoat/discord/teams accounts so first i need to select which account i want to see and also a way to add new accounts. i also want my general! When i click on them and then have i want to have a server overview i can select my favourite servers also a tab where i can add a friend to my favourited friends from that account/server.
Also i need an icon for each and every account i added wich by clicking on it gives me my friendlist and serverlist WITH ICONS for that account, preferably searchable.
Same for discord, same for matrix.
i also need to be able to configure my storage backends.
in the local surealdb it's ok if the account tokens/cookies after the matrix/stoat/discord oauths are stored unencrypted. when synchronized to a backup server they should NEVER reach it unencrypted. here we use our public key username to identify our records in the backup server and check for new settings stored on another account aswell as push our own that haven't been synced yet.

settings are mostly the accounts we're logged into through our messenger.

When i launch the App the very first time i need to have a setup dialog, similar to sessions dialog where i just get my username (public key) and the private mnemonic key pass phrase i should save/export somewhere as a file but will also be my user record in my db. i should always in the settings page for this app also be able to save/export that key again, since this is what we use to decrypt incoming settings changes from the backup server, for when we initialy open the app or do a settings change and need to encrypt that. as i said, same way session messenger oes it.

However mostly since we are a 'poly' messenger app we see are a multi-backend messenger client that handles 'server' style apps like discord/stoat/teams correctly.
remember, we also want to handle selfhosted stoat messenger instances instances. just for good taste probably ALSO self hosted discord instances with servers on them (even though thous shouldn't exist but lets show it that way anyway, sicne anyone can clone their api technically) and obviously any federated matrix server. show matrix.org by default but we probably should be able to get a good list of big public servers somewhere that we can add here.

we need the usual chat convenience features like a send button and sending messenges to work. but also we need to support the oauth/login flows for stoat/matrix/discord when we add a server.
this app is like thunderbird in this way with google/o365 backends.

we also want notifications for friend requests from each account where logged into, see their users and be able to chat with this. 
this whole thing here is simillar to a matrix client, just with the additional to backends of stoat + discord but a view that looks like it's discord.
it needs to look + FEEL kinda like discord, becaue the goal ist to pull my friend who is stuck on discord out of discord only by giving him a client that speak stoat/discord/matrix.

we also on every single platform need to support voice and video calls. you should write in the agents.md to look up flutter packages with native bindings that can help us achieve our goals.

also we are in the age of AI. 
i did not give you a lot of information about the teams backend, i know. let's treat teams group (chats with multiple users) as something that is under direct messages with the teams icon as the source, just like it would be for an indivudal user. teams groups should become servers like with discord/stoat. matrix i already told you.