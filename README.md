# Lightyear Menu 

This code stands as a starting point for a multiplayer bevy steam game using the [lightyear](https://github.com/cBournhonesque/lightyear) networking library. Where a server is ran in the background (separate thread) and can have anyone connect through steam p2p, or Udp. The game is always running as a client, and either connects to the background server via crossbeamIO in lightyear or connects to someone else's server. Server starting and stopping is orchestrated through another set of crossbeam channels which communicate client_commands, and server_commands.


The menu code follows Bevy's menu example


# Working Part

```cargo run -- client``` 
only runs client code (so you have to join a server, you can't press play)

```cargo run -- server``` 
only runs server code in terminal, and auto starts server

```cargo run -- full``` 
starts a client and server, which communicate via crossbeam messages





