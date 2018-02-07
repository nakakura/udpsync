#!/usr/bin/ruby

# _*_ coding: utf-8 _*_
require 'kconv'
require "time"


require "socket"


p "port: " + ARGV[0]

udps = UDPSocket.open()

udps.bind("0.0.0.0", ARGV[0])

loop do
  recv_time = udps.recv(65535).toutf8
  t = Time.now
  nano = t.nsec
  local_time = t.to_i.to_f + t.nsec.to_f / 1000 / 1000 / 1000
  p (local_time.to_f - recv_time.to_f)
end

udps.close


