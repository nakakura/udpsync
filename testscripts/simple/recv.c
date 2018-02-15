#include <stdio.h>
#include <sys/types.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <string.h>

struct CPacket {
	unsigned int ts;
	unsigned long pts;
};

int main()
{
	 int sock;
	  struct sockaddr_in addr;

	   char buf[2048];

	    sock = socket(AF_INET, SOCK_DGRAM, 0);

	     addr.sin_family = AF_INET;
	      addr.sin_port = htons(12345);
	       addr.sin_addr.s_addr = INADDR_ANY;

	        bind(sock, (struct sockaddr *)&addr, sizeof(addr));

	   for(;;){
		 memset(buf, 0, sizeof(buf));

		  recv(sock, buf, sizeof(buf), 0);


		  struct CPacket pack;
		  memcpy(&pack, &buf, sizeof(unsigned int) + sizeof(unsigned long));
		   printf("%u, %ld\n", pack.ts, pack.pts);
	   }
		    close(sock);

		     return 0;
}


