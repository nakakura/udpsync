#!/bin/sh
gst-launch-1.0 udpsrc port=10000 ! application/x-rtp ! rtpjitterbuffer ! rtph264depay ! h264parse !  avdec_h264 ! videorate ! videoconvert ! autovideosink 
