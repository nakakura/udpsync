#!/bin/sh
export OF=/home/nakakura/Desktop/udpsync/red.png
gst-launch-1.0 -v v4l2src device=/dev/video0 ! video/x-raw,framerate=60/1,width=1280,height=720 ! gdkpixbufoverlay location=$OF alpha=1 relative-x=0.5 relative-y=0.5 ! timeoverlay halignment=center valignment=bottom text="Stream time:" shaded-background=true font-desc="Sans, 24"   ! videoconvert ! x264enc bitrate=6000 speed-preset=superfast pass=cbr tune=zerolatency sync-lookahead=0 rc-lookahead=0 sliced-threads=true ! rtph264pay ! udpsink host=127.0.0.1 port=10000 sync=false
