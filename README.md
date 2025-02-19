# Lightyear Menu 

The menu code follows Bevy's menu example

Pressing Play will start the server with Netcode and Steam.

Pressing Join Server will display all friends currently playing your game (If you set up the app_id correctly, presently using 480 as the app id), and pressing their names will make you join their server.

You can also connect via netcode by typing an address in the text box and pressing connect. If you are testing over different computers there is a flag in main.rs. Change local_testing to false.
