# _*_ coding: utf-8 _*_
require 'kconv'
require "socket"

p "IP: " + ARGV[0]
p "port: " + ARGV[1]
p "prefix: " + ARGV[2]

udp = UDPSocket.open()
sockaddr = Socket.pack_sockaddr_in(ARGV[1], ARGV[0])

loop do
    for num in 1..10 do
        udp.send(ARGV[2] + "_HELLO_" + num.to_s , 0, sockaddr)
        udp.send(ARGV[2] + "_あいうえお_" + num.to_s, 0, sockaddr)
        udp.send(ARGV[2] + "_漢字_" + num.to_s, 0, sockaddr)
    	sleep 0.5
    end
end

udp.close
