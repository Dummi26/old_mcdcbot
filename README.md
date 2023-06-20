# mcdcbot
Discord bot for your minecraft server (start/stop/switch configs/chat sync)

## Running

You need to set a few environment variables to get going:

- `mcdcbot_token` contains your discord bot's token
- `mcdcbot_id_report`, `mcdcbot_id_status`, and `mcdcbot_id_chat` are ids of discord text channels
- `mcdcbot_servers` is the path to a UTF-8 text file containing your servers
- `mcdcbot_server_default` is the id of the config you want the bot to use on startup (optional)

## Server config files

You can have multiple servers.
Each of these is a directory containing the executable jar file, server.properties, world/, and whatever else your server needs.

The actual contents of these directories don't matter to the bot, it just needs some directory containing a jar file.

For each server, add this to your config file:

- the header line: an id (without spaces) followed by a space, then the display name (may contain spaces)
- config options (some optional, some required)
  + `type` is the kind of server you are running. this is required because mcdcbot works by reading the server's stdout, which isn't the same for all servers.
    * `vanilla-mojang` for normal servers.
    * `vanilla-papermc` if using papermc.
  + `dir` is the working directory for your jar file (shouldn't end with a `/`!)
  + `exec` is the name of your jar file
  + `ram` (default: 1024) is the amount of ram your server should use in MiB (-Xms<ram>M and -Xmx<ram>M)
  + `java_cmd` (default: unspecified, use `java` from path) can optionally be used to override the java executable (for systems using Java 8, which require a special executable for Java 11 or newer versions in general)
- an empty line before the next header line (optional if the file ends after this config)

for example:

    survival-omi Survival bei der Omi
    type=vanilla-mojang
    dir=/run/media/mark/mcsrv/minecraft_server/survival server bei der omi
    exec=paper-1.19-81.jar
    ram=2048

    survival-cherry Survival with cherries
    type=vanilla-papermc
    dir=/run/media/mark/mcsrv/minecraft_server/survival_cherry
    exec=server.jar
    ram=2048

## Post-Start

If the bot starts correctly, a message will appear in the **report channel**.

The **status channel** can be used to control the minecraft server.
Be careful, only allow trusted users to send messages to this channel!

Available commands are: (this isn't using slash commands right now)

- mc..restart
  + exits the program. this is called restart because usually the bot will restart since
    1. it is started through a shell script that loops infinitely
    2. systemd restarts failing services
    3. usually, to avoid downtime, there will be some mechanism to restart the bot after it exits
- mc.start
  + starts the minecraft server (depending on the selected mode/config)
  + the reply to mc.start will periodically be updated to show the current ip, who is online, system memory usage and load averages.
- mc.stop
  + stops the server again (usually by writing "stop" to its stdin)
- mc.setmode <mode>
  + sets the server mode/config using an id from the servers config file. for invalid ids, lists all valid ids.
- mc.run <command>
  + runs the command by writing it to stdin. useful so people can whitelist themselves.
- mc.status
  + sends a small status message

If a player on the server sends a message, the bot will forward it to the **chat channel**.

If a user sends a message to the **chat channel**, the bot will forward it to the server's chat using /tellraw (NOTE: this may be exploitable!)
