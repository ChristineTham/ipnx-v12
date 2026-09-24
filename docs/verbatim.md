# What Christine said — verbatim

**Every quote in this repository that is attributed to Christine**, extracted
mechanically on 2026-09-17: an italic-quoted passage whose preceding text names
her as the speaker. Quotes of Plan 9's papers and manuals are excluded — an
earlier pass caught those too, which would have been the same false attribution
this document exists to undo.

**This is the only part of the documentation that is hers.** Everything around
these lines — the decisions drawn from them, the designs built on them, the
rules inferred from them — was written by Claude. Measured 2026-09-17: all 31
commits to the decision log, all 44 to the plan, all 20 to CLAUDE.md and all 21
to RESEARCH.md are Claude's, and she has not seen most of it.

So a quote here is evidence of what she said. It is **not** evidence that what a
document concluded from it is what she meant — several times today it was not.
**Where a quote and a conclusion drawn from it disagree, the quote wins.**

Dates are the nearest preceding date in the source: they locate a quote, they
do not certify it.

---


## 2026-08-29

- *"It's like a game demo. A lousy game demo means no one will buy the game."*
  <br>— cited in `docs/archive/design-log-claude-written.md`

## 2026-08-31

- *"I think we may need to retire /mnt/acme."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"IPNX is still there, it is still the userspace and kernel. Saranos is what goes on the top — a user experience."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"Opening a recipe shows the recipe and launches a recipe manager — one of its buttons is New that instantiates a process with that recipe."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"This is the only solution that fits the principle (everything is a file, per process namespace, and 9P is the only protocol)."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"Welcome to Saranos, a modern operating system based on IPNX - a reimagining of UNIX."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"What we have been designing is not acme, or a replacement for acme. It is the shell that IPNX boots into, it is the IPNX primary user interface, it is the browser surface, the macos app, the ios app. The system boots into emca. Emca is the user interface."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"You are trying to push everything through a protocol that should have been an exception rather than the rule."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"any program can write to /dev/window, and in fact it is how emca operates"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"easy to pronounce, mixed etymology, it sounds like a word but isn't, and conveys serenity"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"emca doesn't really need to implement a WYSIWYG editor on the IPNX side. It can implement sam, a batch editor. The job of emca is to push a file into a window via /dev/canvas. The host side can display and scroll the file, and more importantly edit it using Monaco or TextEdit or similar."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"not exposing kitty's credentials in plaintext"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"saranos is the name of the operating system, emca is the windowing and UI system, IPNX is the kernel and userspace."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"the same directory can be an `ls` window or, if you want to edit the listing, an `edit` window"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"we can't just let host edit and send the completed file to emca. We must notify emca of every edit, so essentially emca and the host are maintaining mirror buffers."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"you need to split emca design into two halves — a half that lives in IPNX, and a half that is native to the surface."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"the window behaves like the terminal pane in VS Code. Command > line editing works, and history works (using arrow keys). In VSCode, all this > is handled by the shell itself (bash, using readline) but in IPNX rc is > blissfully unaware, so the host does it."*
  <br>— cited in `docs/userland.md`

## 2026-09-01

- *"Saranos as an OPERATING SYSTEM encompasses host side and WASM side as well… that's why it's different from IPNX, which only describes the kernel and userspace, and that's why Saranos is a different name. It is a symbiosis between host and WASM, neither can exist without the other."*
  <br>— cited in `CLAUDE.md`
- *"The floating toolbar is still the floating toolbar, so it is context sensitive to the window body."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"They are not, all the window controls need to be available at all times, they are part of the UI."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"We already have a kernel in Rust that compiles to WASM, clearly Rust is in the system. Creating a Rust package is a to do."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"We need a compositor - something that arranges windows into columns and rows, and it is recursive. each window itself is a compositor that can further decompose into windows... The entire browser surface (or macos/ios screen) starts off as one giant window, of type root."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"Where you have been confused (judging by you naming panes as Rail, Transcript etc) is that panes are not special, they are just normal windows"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"a window with minimised rows still take up space (one line per row). so maximising a window may not actually give much extra room."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"acme is Bell Labs program. We are going to update it to fit emca, but not change functionality. emca is effectively our new windowing system and UI. Don't confuse between the two."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"acme names for the builtins are idiosyncratic (snarf, zerox, put, get, etc.). They have not stood the test of time, and are against Apple HIG… This only applies to emca, not acme. Acme of course retains it's naming."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"how do we undo a process kill?"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"if we are adopting a stack metaphor, then minimise should be minimising into a stack"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"open and find should always apply… Both should always be true."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"open is open a new window with tagline as title"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"why can't goto and search be the same operation?"*
  <br>— cited in `docs/archive/design-log-claude-written.md`

## 2026-09-02

- *"The decision itself moves off the proposal — so I am reviewing genuine open decisions rather than settled decisions."*
  <br>— cited in `CLAUDE.md`
- *"everything that we have not explicitly discussed and endorsed should be a gap (or proposed if you have created a design). proposed designs need to be reviewed."*
  <br>— cited in `CLAUDE.md`
- *"A shared rasteriser, or native per host?"*
  <br>— cited in `RESEARCH.md`
- *"Research existing package implementations before answering… That will tell you what is needed, rather than me guessing on your behalf."*
  <br>— cited in `RESEARCH.md`
- *"`plan9/` … **Check claims against this one**"*
  <br>— cited in `RESEARCH.md`
- *"`sys/src/cmd/ramfs.c` (907 lines)"*
  <br>— cited in `RESEARCH.md`
- *"do not put configuration into a root that means something else"*
  <br>— cited in `RESEARCH.md`
- *"similarly for draw. we said we would replace by `/dev/canvas`."*
  <br>— cited in `RESEARCH.md`
- *"we definitely had this conversation before."*
  <br>— cited in `RESEARCH.md`
- *"acme, the original Bell Labs program, is an emca-like program using `/dev/draw`, running under emca."*
  <br>— cited in `docs/acme.md`
- *"/usr/kitty is an identity?"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"/usr/kitty/home is a synonym for /home?"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"I do understand pkg needs to wrap up a collection of files but that is not what the package file actually is"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"I think the genuine solution to this is that the '/' type and the screen is genuinely special, it is not a normal window. That's an unescapable fact."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"Only saranos knows about the host… I am a macos app. I have a screen, keyboard and mouse. I will serve these as virtual devices to the IPNX kernel."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"The default type is 'text' which you have been calling. But edit is really the manager of text."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"The kernel is completely out of this."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"X is speced, Y is proposed and Z is gap. Would you like me to review Y with you before implementing, and would you like me to propose Z, before we implement."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"You are conflating a description of saranos and components, which should live in a separate file… next you are describing a spec as a transcript of what we discussed rather than as a spec document. Next you are evaluating what has been built vs what was designed. That does not belong in a spec document."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"`/template/system` is the template. The `/` namespace is a 'project' instantiated from that template. Once instantiated, it can be modified, and saved as new template. You should not be consulting `/template` at boot, the template configuration file is stored in the `/` folder itself. So you should be reading `/namespace` or equivalent — that can be done by the host, and used to create the rest of `/`."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"a base, a per-device section, a section per service"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"a template really is a proto project… What about .gitignore, README, package.json?"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"a window type is encapsulating things that are not text, that's why we need a manager, which understands how to render/edit the type, knows what to do with the status line, supplies toolbar buttons, etc."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"a window's content is always a file, but the host may choose not to render it as a file but as an image, structured/formatted text, a table, etc."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"accept your proposals. review and update all documentation to reflect agreed design. Make sure stale decisions that have been overruled are not still there to confuse future readers."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"acme… is an emca like program using /dev/draw running under emca"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"and `/home/bin`, etc. all standard conventions."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"complexity is compensation"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"duplicate is three buttons on our current emca implementation but may change."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"emca is a window manager. it controls the placement of windows on the screen. a type manager controls what is in a window… type managers may communicate with window managers (over 9P of course)."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"file type not displayable, want me to display as text?"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"home is a workspace."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"if there is a system /profile, then the user profile should genuinely be /home/profile. Which means we are back to /usr/kitty being a synonym for /home."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"in full alignment with Unix philosophy"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"may be commands outside manager, eg. a shell"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"new rows and new columns should only ever be created by a user, so an open window opens a new tab by default… It is also the safest option, that does not destroy current layout."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"pkg is not a directory. it is a list of bindings… plus commands that may need to be invoked during install… It is actually very similar to template."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"shell on a file is really append a shell conversation to the file… I can open a log file, and append to it. In other words, treat the file as a pseudo `/dev/cons`."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"sudo mk install works exactly as we would imagine."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"the conversation can be displayed as a file though"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"the kernel should not be involved at all"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"the shell breaks the convention that everything is a file. It is a conversation, not a file"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"the store must be prunable."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"this is what convention buys. someone creating a template in `/home/project` is an idiot."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"walks try elements in order, directory reads concatenate integrally, **creates land in the MCREATE element**."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"we could instantiate a new emca in a window - it will open '/', does a layout in that window"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"you could argue there are system `/profile` and system `/credentials`."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"you did not name Add in our discussions"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"you mentioned that /output could be served by emca which means its /output/XXX is the shortest where XXX could be just a number."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"Unix commands are binary executables. /dev/kmem is binary. /etc/passwd is a text file, but it is a structured text file."*
  <br>— cited in `docs/verbatim.md`
- *"We need to land on precisely what the window controls are and what the standard toolbar buttons are. We cannot have inconsistency."*
  <br>— cited in `docs/verbatim.md`
- *"su mimmy is 'Become mimmy, create a process with /usr/mimmy mapped to home and mimmy's credentials'."*
  <br>— cited in `docs/verbatim.md`
- *"su. followed by pkg install installs a package into the system namespace, which is inherited by every user… su creates a process where we can change the system namespace, credentials and profile"*
  <br>— cited in `docs/verbatim.md`
- *"the convention is `/dev` can be virtualised."*
  <br>— cited in `docs/window.md`

## 2026-09-03

- *"Actual deviations … are only authorised when it is to do with adapting it for WASM and WASI"*
  <br>— cited in `CLAUDE.md`
- *"even then it should be done in a machine independent way as we may want a non WASM kernel in the future … for example, dis, or .NET CLR"*
  <br>— cited in `CLAUDE.md`
- *"we are essentially implementing a micro kernel based on a subset of Plan 9, we should not be adding to it (even the Unix v10 personality should be userspace)"*
  <br>— cited in `CLAUDE.md`
- *"you yourself said the kernel does not grow. The kernel only handles process orchestration. everything else is handled by host or userspace. Everytime you design a change to the kernel, the design is wrong."*
  <br>— cited in `CLAUDE.md`
- *"It sounds like you have strayed a lot from original plan 9 design. How does plan9 handle the root filesystem if ramfs is userspace?"*
  <br>— cited in `RESEARCH.md`
- *"`/dev/draw` should be rendered by host. the kernel does not know how to draw"*
  <br>— cited in `RESEARCH.md`
- *"only when it is to do with adapting it for WASM and WASI"*
  <br>— cited in `RESEARCH.md`
- *"the kernel only handles process orchestration; everything else is handled by host or userspace"*
  <br>— cited in `RESEARCH.md`
- *"the kernel was supposed to be a reimplementation of a subset of plan 9 kernel. it sounds like you have broken the contract. that needs to be rectified completely."*
  <br>— cited in `RESEARCH.md`
- *"we are essentially implementing a micro kernel based on a subset of Plan 9, we should not be adding to it (even the Unix v10 personality should be userspace) … It is important to keep our kernel pure otherwise we will encounter serious issues extending the kernel"*
  <br>— cited in `RESEARCH.md`
- *"/dev/draw should be rendered by host. the kernel does not know how to draw."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"Actual deviations from Plan 9 kernel are only authorised when it is to do with adapting it for WASM and WASI"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"accept the proposal, then delete the kernel's tree."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"even then it should be done in a machine independent way as we may want a non WASM kernel in the future … for example, dis, or .NET CLR etc."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"if it is global you need to resolve collision."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"we don't need /dev/draw for the demo, text is sent to host, which is responsible for rendering. /dev/draw is only needed for acme."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"we don't use /dev/draw — we use /dev/canvas"*
  <br>— cited in `docs/archive/design-log-claude-written.md`

## 2026-09-04

- *"You already know the answer. Can you check you are not making the same mistakes elsewhere in the plan?"*
  <br>— cited in `RESEARCH.md`
- *"doesn't /rc contain boot sequence and command?"*
  <br>— cited in `RESEARCH.md`
- *"**not that we should implement a bootloader**"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"I noticed you have been adding steps (M17, M18) rather than realising earlier phases have been invalidated by design decisions, so need to be redone. So let's replan properly, and restart implementation rather than continuing."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"What I meant was we can't call something /boot and refer to something other than a bootloader"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"`/boot/boot` is wrong I've already said that is bootloader by convention"*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"a plan that gradually implements IPNX (kernel and userspace), then emca, then Saranos."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"we are talking about the system default namespace and profile. Since this is exactly what a template is (specifies a namespace) the system boot path is the template configuration file for `/` (think of `/` as a 'project' that has been instantiated from the 'system' template which should live in `/template/system`)."*
  <br>— cited in `docs/archive/design-log-claude-written.md`
- *"a store entry never changes after verification"*
  <br>— cited in `docs/type.md`

## 2026-09-15

- *"yes, create a tree form"*
  <br>— cited in `docs/archive/design-log-claude-written.md`

## 2026-09-18

On the window type system and manager interface proposal, answering its five
open questions in order:

- *"1. yes for now until we find an issue. 2. yes. 3. per window. 4. host half lives in saranos app (swiftui, browser app via node etc.) 5. declare verb list"*

On canvas and draw:

- *"On canvas.md - we created it because /dev/draw was bitmapped and we wanted a vector canvas. Why can't we leverage as much of Plan 9 as possible? Also we can implement /dev/svg if we want SVG semantics. the rest are approved"*

On `#c` and `#¤`:

- selected *"Both in, as Plan 9 has them"* — the kernel carries Plan 9's device set except `#i` draw and `#m` mouse.

On method:

- *"It seems to be you actually have a easily easy way to to verify your design documents. What does Plan 9 do? That should answer most of your questions"*
- *"README was never mine - I have already said nothing in this repo was authored by me. All I did was edited a few paragraphs. Your sudden decision that it is now my document that you cannot touch was your invention, never authorised by me"*
- *"Why do you have to keep reverting to history and explaining everything. Why can't we just document the current design and current status?"*
- *"you should be committing to main"*

On `#c`, after it was removed on the process-orchestration rule:

- *"we should keep #c in since it holds a variety of kernel info"*

On implementing the rest of `#c`:

- *"implement rest of #c to the best of your ability. No reason why we can't do swap, config, reboot. It can also report drivers provided by the host app"*
- *"remember just because the functionality is provided by host app doesn't mean it can't be visible and reported by kernel"*

Clarifying the kernel-does-not-grow rule, after it was read as a ban on
everything but process management:

- *"you are interpreting 'only process orchestration in kernel' - that rule was to stop you from adding all sorts of invented stuff in the kernel. the kernel focuses on the one thing it does best - process orchestration, but it doesn't mean that is the only thing the kernel does. Use Plan 9 as a guide."*

Proposing the rule that opens the Conventions section, after a day of
deviations:

- *"given all the deviations so far, why can't you introduce a rule that says before you implement anything, you must consult Plan 9 source and you are not allowed to deviate"*

Rejecting the hedge in the first draft of that rule:

- *"That cannot be right. You said you did not look at struct Dev. That would not have happened if you were forced and no deviations allowed"*

## 2026-09-24

On P7's *"`/profile`, `/pkg` and `/template` as one format, three
registries"*:

- *"I am questioning "one format, three registries" - either these are all the same or they are different thing. In my original concept they are completely different"*
  <br>— cited in `docs/implementation.md`

What each of them is:

- *"To me a pkg is like a FreeBSD pkg or apt or any other package manager. It contains a list files to be bound in the namespace, plus potentially initialisation scripts (write out config files, set out environment etc.)."*
- *"A template is a prototype for a project (ie. a NodeJS project, a Python project) - it may install packages, but contains project scaffolding. eg. package.jso, gitignore, editor config files etc, sample code. The key difference between a template and a pkg is that a templates instantiates new versions of files (scaffolding), not just binds of files shared across namespaces."*
- *"A profile is how a user wants their namespace organised, user config files, environment variables, login scripts etc."*
- *"A profile may be built from a template, but essentially once it is instantiated it belongs to the user and up to the user to modify"*
  <br>— cited in `docs/implementation.md`

Answering what a package's scripts change, what a template records, and what
a profile is built from:

- *"installing a package may modify a user's profile (add environment variables, execute scripts etc.). It can also alter the system's environment (/rc, /profile) etc. Think of it as apt install. a package install can be to the system (available to every user, process etc.) or to the namespace (only valid for current process) or to the user (available whenever the user logs in, in every user process)."*
- *"a template install packages into the current project, so is persistent. opening a project ensures all packages are available."*
- *"A profile can be built from a template, or user hand editing config files. The user can save current namespace config as a template for future profiles"*
  <br>— cited in `docs/proposals.md`

On servers, and on what a project is:

- *"packages can also install servers/daemons - these can be system, user or project specific. Example, installing PostgreSQL or MongoDB - into system starts a server when system starts, configurable in /rc. In user, starts when user logs in, terminates when user logs out. Project - starts when project is opened, terminates when project is closed."*
- *"This practically means a project is a type that is instantiated when user opens project file in a new emca window. Projects live in /project/x but the binding is user/process speccific. for 2 different users, it could be two projects. Or alternatively two users share a project."*
- *"Use existing package managers as an inspiration for packages primitives"*
- *"Sorry you are right - services and packages should be different. maybe we should use different specs for them"*
  <br>— cited in `docs/proposals.md`
- *"a package should be like installing a toolchain or a library, services installs daemons"*
  <br>— cited in `docs/proposals.md`
- *"what happened to /profile"*
- *"I think we are getting confused between similar things. I envisaged /profile to contain any files required to configure a system - the kind of stuff in Unix /etc. network config, namespace bindings, init scripts etc. The user's profile is contained in /home (synonym for /usr/<username>), in /home/profile. So the info in /rc probably should be in /profile and we should retire the concept of /rc. Now I am thinking /pkg contains a list of packages installed. /service contains a list of services installed etc. /template has a list of templates etc."*
  <br>— cited in `docs/proposals.md`
- *"I don't understand /rc/bin? Why are these not stored in /store and bound to /bin like a package? rc should have two startup files - a system one (which is in /profile) and a user one (in /home/profile)"*
  <br>— cited in `docs/proposals.md`
- *"should not call them rcmain since that is confusing. I am not liking the plan 9 names (termrc, cpurc) so why not name them after saranos terms (systemrc, userrc, servicerc, pkgrc, emcarc) in appropriate folders?"*
  <br>— cited in `docs/proposals.md`
- *"I am thinking we should actually name them by role rather than distinguishing between system and user. How about: /profile/startrc - executes when system boots /profile/shellrc - executes with every new shell /profile/stoprc - execute when system shuts down User equivalents are in /home/profile, /service/<service name> etc. Packages and templates don"* — the message ends there
  <br>— cited in `docs/proposals.md`
- *"Packages and templates have configrc instead"* — completing the message above
  <br>— cited in `docs/proposals.md`
- *"or maybe installrc and removerc"* — for packages and templates, in place of `configrc`
  <br>— cited in `docs/proposals.md`
