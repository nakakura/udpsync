require 'socket'

sender    = UDPSocket.new

s    = UDPSocket.new
port = ARGV[2]
s.bind("0.0.0.0", port)

while true
  ar = s.recvfrom(65535)
  sender.send(ar[0], 0, ARGV[0], ARGV[1])
end
