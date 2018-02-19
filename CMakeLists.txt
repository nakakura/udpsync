cmake_minimum_required(VERSION 3.9)
project(udpsync)

set(CMAKE_CXX_STANDARD 11)

add_executable(udpsync
        src/bin/receiver.rs
        src/bin/sender.rs
        src/buffer.rs
        src/gstreamer_mock.rs
        src/haptic_data.rs
        src/keyboard.rs
        src/lib.rs
        src/rtp.c
        src/rtp.rs
        src/udp.rs
        testscripts/latency/recv.rb
        testscripts/latency/send.rb
        testscripts/simple/a.out
        testscripts/simple/hoge.txt
        testscripts/simple/recv.c
        testscripts/simple/recv.rb
        testscripts/simple/redirect.rb
        testscripts/simple/send.rb
        testscripts/test.py
        testscripts/test.rb
        build.rs
        camera_overray_local.sh
        camera_overray_remote.sh
        camera_send_local.sh
        camera_send_remote.sh
        Cargo.lock
        Cargo.toml
        flow.png
        flow.xml
        gstgdkpixbufoverlay.c
        hoge
        play.sh
        recv_rtp.sh
        red.png
        send_local.sh
        send_rtp.sh
        test.csv)
