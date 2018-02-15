require 'socket'

target = "127.0.0.1"
port = 10000
sender    = UDPSocket.new

s    = UDPSocket.new
port = 9000
s.bind("0.0.0.0", port)

while true
  ar = s.recvfrom(65535)
  sender.send(ar[0], 0, target, 10000)
end
