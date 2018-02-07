# _*_ coding: utf-8 _*_
require 'kconv'
require "socket"
require "time"

p "IP: " + ARGV[0]
p "port: " + ARGV[1]

udp = UDPSocket.open()
sockaddr = Socket.pack_sockaddr_in(ARGV[1], ARGV[0])

loop do
  t = Time.now
  nano = t.nsec
  timestamp = t.to_i.to_f + t.nsec.to_f / 1000 / 1000 / 1000
  udp.send(timestamp.to_s, 0, sockaddr)
  sleep 0.9 
end

udp.close
