#!/bin/sh
gst-launch-1.0 -v v4l2src device=/dev/video1 ! video/x-raw,framerate=60/1,width=1280,height=720 ! videoconvert ! x264enc bitrate=6000 speed-preset=superfast pass=cbr tune=zerolatency sync-lookahead=0 rc-lookahead=0 sliced-threads=true ! rtph264pay ! udpsink host=127.0.0.1 port=10000 sync=false
