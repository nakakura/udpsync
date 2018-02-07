#!/usr/bin/ruby

require "socket"

udps = UDPSocket.open()

udps.bind("0.0.0.0", 30001)

loop do
  p udps.recv(65535)
end

udps.close

