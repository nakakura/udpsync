# _*_ coding: utf-8 _*_
require 'kconv'

#!/usr/bin/ruby

require "socket"


p "port: " + ARGV[0]

udps = UDPSocket.open()

udps.bind("0.0.0.0", ARGV[0])

loop do
    p udps.recv(65535).toutf8
end
udps.close


