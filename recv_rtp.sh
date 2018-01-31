#!/bin/sh
gst-launch-1.0 udpsrc port=61234 ! application/x-rtp ! rtpjitterbuffer ! rtph264depay ! h264parse !  avdec_h264 ! videorate ! videoconvert ! autovideosink sync=false
