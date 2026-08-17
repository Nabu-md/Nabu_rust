ranslated Report (Full Report Below)
-------------------------------------

Process:               app [21869]
Path:                  /Applications/Nabu.app/Contents/MacOS/app
Identifier:            md.nabu.app
Version:               0.1.0 (0.1.0)
Code Type:             X86-64 (Native)
Parent Process:        launchd [1]
User ID:               501

Date/Time:             2026-08-17 01:30:36.1195 -0600
OS Version:            macOS 15.0 (24A335)
Report Version:        12
Bridge OS Version:     9.0 (22P353)
Anonymous UUID:        155BD1B8-6489-F5E1-B1CC-D3DC2A035CCC

Sleep/Wake UUID:       C291AF2C-6427-42B4-ADF4-E4241EF5FEB3

Time Awake Since Boot: 4000000 seconds
Time Since Wake:       18692 seconds

System Integrity Protection: enabled

Crashed Thread:        0  main  Dispatch queue: com.apple.main-thread

Exception Type:        EXC_CRASH (SIGABRT)
Exception Codes:       0x0000000000000000, 0x0000000000000000

Termination Reason:    Namespace SIGNAL, Code 6 Abort trap: 6
Terminating Process:   app [21869]

Application Specific Information:
abort() called


Kernel Triage:
VM - (arg = 0x3) mach_vm_allocate_kernel failed within call to vm_map_enter
VM - (arg = 0x3) mach_vm_allocate_kernel failed within call to vm_map_enter


Thread 0 Crashed:: main Dispatch queue: com.apple.main-thread
0   libsystem_kernel.dylib        	    0x7ff808470b52 __pthread_kill + 10
1   libsystem_pthread.dylib       	    0x7ff8084aaf85 pthread_kill + 262
2   libsystem_c.dylib             	    0x7ff8083cbb19 abort + 126
3   app                           	       0x10d7acc69 _RNvNtNtNtCsgejaSCmAoRz_3std3sys3pal4unix14abort_internal + 9
4   app                           	       0x10d7aca39 _RNvNtCsgejaSCmAoRz_3std7process5abort + 9
5   app                           	       0x10d722449 _RNvNtCsgejaSCmAoRz_3std9panicking15panic_with_hook + 892
6   app                           	       0x10d708552 _RNCNvNtCsgejaSCmAoRz_3std9panicking13panic_handler0B5_ + 114
7   app                           	       0x10d6fc889 _RINvNtNtCsgejaSCmAoRz_3std3sys9backtrace26___rust_end_short_backtraceNCNvNtB6_9panicking13panic_handler0zEB6_ + 9
8   app                           	       0x10d708f24 _RNvCs9wFQrvczXsK_7___rustc17rust_begin_unwind + 36
9   app                           	       0x10d7adaac _RNvNtCsbAqs9W1eE8G_4core9panicking18panic_nounwind_fmt + 44
10  app                           	       0x10d7ada17 _RNvNtCsbAqs9W1eE8G_4core9panicking14panic_nounwind + 23
11  app                           	       0x10d7adbb2 _RNvNtCsbAqs9W1eE8G_4core9panicking19panic_cannot_unwind + 19
12  app                           	       0x10cdbe13d _RNvNtNtNtCslaF18iYGa9m_3tao13platform_impl8platform12app_delegate20did_finish_launching + 301
13  CoreFoundation                	    0x7ff80858606c __CFNOTIFICATIONCENTER_IS_CALLING_OUT_TO_AN_OBSERVER__ + 137
14  CoreFoundation                	    0x7ff808613b42 ___CFXRegistrationPost_block_invoke + 88
15  CoreFoundation                	    0x7ff808613a91 _CFXRegistrationPost + 530
16  CoreFoundation                	    0x7ff8085557ac _CFXNotificationPost + 765
17  Foundation                    	    0x7ff809562bab -[NSNotificationCenter postNotificationName:object:userInfo:] + 82
18  AppKit                        	    0x7ff80becc7b5 -[NSApplication _postDidFinishNotification] + 311
19  AppKit                        	    0x7ff80becc4fa -[NSApplication _sendFinishLaunchingNotification] + 215
20  AppKit                        	    0x7ff80beca4a0 -[NSApplication(NSAppleEventHandling) _handleAEOpenEvent:] + 542
21  AppKit                        	    0x7ff80beca0f3 -[NSApplication(NSAppleEventHandling) _handleCoreEvent:withReplyEvent:] + 679
22  Foundation                    	    0x7ff80958bc11 -[NSAppleEventManager dispatchRawAppleEvent:withRawReply:handlerRefCon:] + 307
23  Foundation                    	    0x7ff80958ba25 _NSAppleEventManagerGenericHandler + 80
24  AE                            	    0x7ff80fb60385 0x7ff80fb55000 + 45957
25  AE                            	    0x7ff80fb5fc13 0x7ff80fb55000 + 44051
26  AE                            	    0x7ff80fb59518 aeProcessAppleEvent + 409
27  HIToolbox                     	    0x7ff813ba7406 AEProcessAppleEvent + 55
28  AppKit                        	    0x7ff80bec30c2 _DPSNextEvent + 1725
29  AppKit                        	    0x7ff80c8df4c8 -[NSApplication(NSEventRouting) _nextEventMatchingEventMask:untilDate:inMode:dequeue:] + 1290
30  AppKit                        	    0x7ff80beb3eb7 -[NSApplication run] + 610
31  app                           	       0x10c93cbf3 _RINvMs3_NtNtNtCslaF18iYGa9m_3tao13platform_impl8platform10event_loopINtB6_9EventLoopINtCsiQj4t0msQ3m_17tauri_runtime_wry7MessageNtCs3ALwlAwKxNk_5tauri16EventLoopMessageEE3runNCINvB1n_18make_event_handlerB22_NCINvMsf_NtB24_3appNtB3s_3App28make_run_event_loop_callbackNCNvCsk0WWsatWZTt_7app_lib3runs2_0E0E0EB4k_ + 595
32  app                           	       0x10c93e04a _RINvMsf_NtCs3ALwlAwKxNk_5tauri3appNtB6_3App3runNCNvCsk0WWsatWZTt_7app_lib3runs2_0EBN_ + 682
33  app                           	       0x10cafc439 _RNvCsk0WWsatWZTt_7app_lib3run + 1897
34  app                           	       0x10c80eaf6 _RINvNtNtCsgejaSCmAoRz_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECsigLMue0smWE_3app + 6
35  app                           	       0x10c80eb0c _RNCINvNtCsgejaSCmAoRz_3std2rt10lang_startuE0CsigLMue0smWE_3app + 12
36  app                           	       0x10d720cdb _RNvNtCsgejaSCmAoRz_3std2rt19lang_start_internal + 875
37  app                           	       0x10c80eb5c main + 44
38  dyld                          	    0x7ff80811d2cd start + 1805

Thread 1:
0   libsystem_pthread.dylib       	    0x7ff8084a6bcc start_wqthread + 0

Thread 2:
0   libsystem_pthread.dylib       	    0x7ff8084a6bcc start_wqthread + 0

Thread 3:
0   libsystem_pthread.dylib       	    0x7ff8084a6bcc start_wqthread + 0

Thread 4:
0   libsystem_pthread.dylib       	    0x7ff8084a6bcc start_wqthread + 0

Thread 5::  Dispatch queue: com.apple.WebKit.ServicesController
0   libsystem_kernel.dylib        	    0x7ff80846b5d2 __ulock_wait + 10
1   libdispatch.dylib             	    0x7ff808303ffa _dlock_wait + 46
2   libdispatch.dylib             	    0x7ff808303e82 _dispatch_thread_event_wait_slow + 40
3   libdispatch.dylib             	    0x7ff808310b6e __DISPATCH_WAIT_FOR_QUEUE__ + 307
4   libdispatch.dylib             	    0x7ff80831079a _dispatch_sync_f_slow + 175
5   WebKit                        	    0x7ff9101bbd48 void std::__1::__call_once_proxy[abi:sn180100]<std::__1::tuple<WebKit::ServicesController::refreshExistingServices(bool)::'block-literal'::$_5&&>>(void*) + 50
6   libc++.1.dylib                	    0x7ff8083df0d8 std::__1::__call_once(unsigned long volatile&, void*, void (*)(void*)) + 146
7   WebKit                        	    0x7ff9101b01a1 invocation function for block in WebKit::ServicesController::refreshExistingServices(bool) + 329
8   libdispatch.dylib             	    0x7ff808302455 _dispatch_call_block_and_release + 12
9   libdispatch.dylib             	    0x7ff8083037e2 _dispatch_client_callout + 8
10  libdispatch.dylib             	    0x7ff80830995b _dispatch_lane_serial_drain + 739
11  libdispatch.dylib             	    0x7ff80830a3e2 _dispatch_lane_invoke + 377
12  libdispatch.dylib             	    0x7ff8083140db _dispatch_root_queue_drain_deferred_wlh + 271
13  libdispatch.dylib             	    0x7ff8083139dc _dispatch_workloop_worker_thread + 659
14  libsystem_pthread.dylib       	    0x7ff8084a7c7f _pthread_wqthread + 326
15  libsystem_pthread.dylib       	    0x7ff8084a6bdb start_wqthread + 15

Thread 6:
0   libsystem_pthread.dylib       	    0x7ff8084a6bcc start_wqthread + 0

Thread 7:: JavaScriptCore libpas scavenger
0   libsystem_kernel.dylib        	    0x7ff80846c9aa __psynch_cvwait + 10
1   libsystem_pthread.dylib       	    0x7ff8084ab7a8 _pthread_cond_wait + 1193
2   JavaScriptCore                	    0x7ff909b4dfa7 scavenger_thread_main + 1799
3   libsystem_pthread.dylib       	    0x7ff8084ab253 _pthread_start + 99
4   libsystem_pthread.dylib       	    0x7ff8084a6bef thread_start + 15

Thread 8:: com.apple.coreanimation.render-server
0   libsystem_kernel.dylib        	    0x7ff808469e0e mach_msg2_trap + 10
1   libsystem_kernel.dylib        	    0x7ff808478622 mach_msg2_internal + 84
2   libsystem_kernel.dylib        	    0x7ff808470f16 mach_msg_overwrite + 649
3   libsystem_kernel.dylib        	    0x7ff80846a0ff mach_msg + 19
4   QuartzCore                    	    0x7ff8114467b1 CA::Render::Server::server_thread(void*) + 863
5   QuartzCore                    	    0x7ff811446443 thread_fun(void*) + 25
6   libsystem_pthread.dylib       	    0x7ff8084ab253 _pthread_start + 99
7   libsystem_pthread.dylib       	    0x7ff8084a6bef thread_start + 15

Thread 9:: WebCore: Scrolling
0   libsystem_kernel.dylib        	    0x7ff808469e0e mach_msg2_trap + 10
1   libsystem_kernel.dylib        	    0x7ff808478622 mach_msg2_internal + 84
2   libsystem_kernel.dylib        	    0x7ff808470f16 mach_msg_overwrite + 649
3   libsystem_kernel.dylib        	    0x7ff80846a0ff mach_msg + 19
4   CoreFoundation                	    0x7ff808590c48 __CFRunLoopServiceMachPort + 143
5   CoreFoundation                	    0x7ff80858f6cd __CFRunLoopRun + 1393
6   CoreFoundation                	    0x7ff80858eb6c CFRunLoopRunSpecific + 536
7   CoreFoundation                	    0x7ff808607e14 CFRunLoopRun + 40
8   JavaScriptCore                	    0x7ff9082c28c2 WTF::Detail::CallableWrapper<WTF::RunLoop::create(WTF::ASCIILiteral, WTF::ThreadType, WTF::Thread::QOS)::$_0, void>::call() + 82
9   JavaScriptCore                	    0x7ff9082e2f1d WTF::Thread::entryPoint(WTF::Thread::NewThreadContext*) + 237
10  JavaScriptCore                	    0x7ff9080d7159 WTF::wtfThreadEntryPoint(void*) + 9
11  libsystem_pthread.dylib       	    0x7ff8084ab253 _pthread_start + 99
12  libsystem_pthread.dylib       	    0x7ff8084a6bef thread_start + 15

Thread 10:
0   libsystem_pthread.dylib       	    0x7ff8084a6bcc start_wqthread + 0

Thread 11:
0   libsystem_pthread.dylib       	    0x7ff8084a6bcc start_wqthread + 0

Thread 12:: CVDisplayLink
0   libsystem_kernel.dylib        	    0x7ff80846c9aa __psynch_cvwait + 10
1   libsystem_pthread.dylib       	    0x7ff8084ab7d9 _pthread_cond_wait + 1242
2   CoreVideo                     	    0x7ff811bfdce5 CVDisplayLink::waitUntil(unsigned long long) + 375
3   CoreVideo                     	    0x7ff811bfcc5e CVDisplayLink::runIOThread() + 526
4   libsystem_pthread.dylib       	    0x7ff8084ab253 _pthread_start + 99
5   libsystem_pthread.dylib       	    0x7ff8084a6bef thread_start + 15


Thread 0 crashed with X86 Thread State (64-bit):
  rax: 0x0000000000000000  rbx: 0x0000000000000006  rcx: 0x00007ff7b36edee8  rdx: 0x0000000000000000
  rdi: 0x0000000000000103  rsi: 0x0000000000000006  rbp: 0x00007ff7b36edf10  rsp: 0x00007ff7b36edee8
   r8: 0x0000000000000003   r9: 0x00007f95d7d13db0  r10: 0x0000000000000000  r11: 0x0000000000000246
  r12: 0x0000600000117cc0  r13: 0x0000000000041400  r14: 0x0000000000000103  r15: 0x0000000000000016
  rip: 0x00007ff808470b52  rfl: 0x0000000000000246  cr2: 0x0000000000000000
  
Logical CPU:     0
Error Code:      0x02000148 
Trap Number:     133


Binary Images:
       0x10c80d000 -        0x10dee4fff md.nabu.app (0.1.0) <1b4fc5d6-6c48-393e-91ff-5b9bcac0ec00> /Applications/Nabu.app/Contents/MacOS/app
       0x11a1d5000 -        0x11a1e1fff libobjc-trampolines.dylib (*) <a732c7f4-a3c1-39e5-9fc3-5e1deb73a584> /usr/lib/libobjc-trampolines.dylib
       0x127d87000 -        0x127e84fff com.apple.AMDRadeonX4000GLDriver (6.1.13) <d3de2094-5363-3ff7-9a1e-3ec562f455c7> /System/Library/Extensions/AMDRadeonX4000GLDriver.bundle/Contents/MacOS/AMDRadeonX4000GLDriver
       0x12aedb000 -        0x12b8b6fff com.apple.audio.codecs.Components (7.0) <b93b4d2e-2550-3682-a891-fe1174c991fa> /System/Library/Components/AudioCodecs.component/Contents/MacOS/AudioCodecs
    0x7ff808469000 -     0x7ff8084a4fff libsystem_kernel.dylib (*) <a0aee5ca-4298-3070-82f9-ea72229f36e5> /usr/lib/system/libsystem_kernel.dylib
    0x7ff8084a5000 -     0x7ff8084b0fff libsystem_pthread.dylib (*) <c0db9cf9-86ec-31d4-a557-2c07945fd8f2> /usr/lib/system/libsystem_pthread.dylib
    0x7ff80834b000 -     0x7ff8083d3ff7 libsystem_c.dylib (*) <2d4e63ef-e31c-3cc1-94ec-2b7e28b9782f> /usr/lib/system/libsystem_c.dylib
    0x7ff808514000 -     0x7ff8089b3ff2 com.apple.CoreFoundation (6.9) <a7324227-eb88-3393-8efe-10a9f3d28064> /System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation
    0x7ff809559000 -     0x7ff80a392ff0 com.apple.Foundation (6.9) <b54a23dd-8603-361b-ad2e-54ac2cd8ac39> /System/Library/Frameworks/Foundation.framework/Versions/C/Foundation
    0x7ff80be83000 -     0x7ff80d31dffd com.apple.AppKit (6.9) <55408426-52c7-3b83-9097-0a12aa2620e1> /System/Library/Frameworks/AppKit.framework/Versions/C/AppKit
    0x7ff80fb55000 -     0x7ff80fbc4fff com.apple.AE (944) <2b604f6e-cdc7-349b-80b5-6bf5faa9a9d2> /System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/AE.framework/Versions/A/AE
    0x7ff813b80000 -     0x7ff813e5bff4 com.apple.HIToolbox (2.1.1) <98a58f35-29b9-32ce-b1ba-5bde0bfe5ae2> /System/Library/Frameworks/Carbon.framework/Versions/A/Frameworks/HIToolbox.framework/Versions/A/HIToolbox
    0x7ff808117000 -     0x7ff8081a332f dyld (*) <e6056c94-fc2d-3517-b1e1-46d8eb58a10e> /usr/lib/dyld
               0x0 - 0xffffffffffffffff ??? (*) <00000000-0000-0000-0000-000000000000> ???
    0x7ff808300000 -     0x7ff808347ff9 libdispatch.dylib (*) <6c0ff4e0-6f75-36fa-b45f-0075a398132d> /usr/lib/system/libdispatch.dylib
    0x7ff90fb8b000 -     0x7ff910a4fff7 com.apple.WebKit (20619) <6e1f3e30-5977-33d3-80cc-96385c75119c> /System/Library/Frameworks/WebKit.framework/Versions/A/WebKit
    0x7ff8083d4000 -     0x7ff808450ffb libc++.1.dylib (*) <e35e82f9-4037-35da-99f0-4d09be1d9721> /usr/lib/libc++.1.dylib
    0x7ff9080d4000 -     0x7ff909d8af65 com.apple.JavaScriptCore (20619) <d02e0fae-3c48-32fe-b010-ac8d1b8c0648> /System/Library/Frameworks/JavaScriptCore.framework/Versions/A/JavaScriptCore
    0x7ff8113fc000 -     0x7ff811798ff2 com.apple.QuartzCore (1.11) <681f04a8-f237-3d8a-a6e3-ea230e0678c5> /System/Library/Frameworks/QuartzCore.framework/Versions/A/QuartzCore
    0x7ff811bfa000 -     0x7ff811c4dff3 com.apple.CoreVideo (1.8) <c07bd761-b2b4-351e-8793-f18b0ad28f2a> /System/Library/Frameworks/CoreVideo.framework/Versions/A/CoreVideo

External Modification Summary:
  Calls made by other processes targeting this process:
    task_for_pid: 0
    thread_create: 0
    thread_set_state: 0
  Calls made by this process:
    task_for_pid: 0
    thread_create: 0
    thread_set_state: 0
  Calls made by all processes on this machine:
    task_for_pid: 121
    thread_create: 0
    thread_set_state: 1786

VM Region Summary:
ReadOnly portion of Libraries: Total=1.2G resident=0K(0%) swapped_out_or_unallocated=1.2G(100%)
Writable regions: Total=1.8G written=0K(0%) resident=0K(0%) swapped_out=0K(0%) unallocated=1.8G(100%)

                                VIRTUAL   REGION 
REGION TYPE                        SIZE    COUNT (non-coalesced) 
===========                     =======  ======= 
Activity Tracing                   256K        1 
ColorSync                          244K       29 
CoreAnimation                      240K       28 
CoreGraphics                        16K        3 
CoreServices                        60K        1 
Foundation                          16K        1 
IOKit                             15.5M        2 
JS JIT generated code              1.0G        3 
Kernel Alloc Once                    8K        1 
MALLOC                           658.4M       69 
MALLOC guard page                   48K       12 
STACK GUARD                         48K       12 
Stack                             14.6M       13 
Stack Guard                       56.0M        1 
VM_ALLOCATE                        208K       16 
VM_ALLOCATE (reserved)             128K        1         reserved VM address space (unallocated)
WebKit Malloc                    160.0M        4 
WebKit Malloc (reserved)          32.0M        1         reserved VM address space (unallocated)
__CTF                               824        1 
__DATA                            30.7M      774 
__DATA_CONST                      94.6M      789 
__DATA_DIRTY                      1989K      277 
__FONT_DATA                        2352        1 
__GLSLBUILTINS                    5174K        1 
__INFO_FILTER                         8        1 
__LINKEDIT                       190.3M        5 
__OBJC_RO                         76.1M        1 
__OBJC_RW                         2354K        2 
__TEXT                             1.0G      804 
__TPRO_CONST                       272K        2 
mapped file                      233.1M       25 
owned unmapped memory              256K        1 
shared memory                      792K       18 
===========                     =======  ======= 
TOTAL                              3.5G     2900 
TOTAL, minus reserved VM space     3.5G     2900 



-----------
Full Report
-----------

{"app_name":"app","timestamp":"2026-08-17 01:30:40.00 -0600","app_version":"0.1.0","slice_uuid":"1b4fc5d6-6c48-393e-91ff-5b9bcac0ec00","build_version":"0.1.0","platform":1,"bundleID":"md.nabu.app","share_with_app_devs":0,"is_first_party":0,"bug_type":"309","os_version":"macOS 15.0 (24A335)","roots_installed":0,"name":"app","incident_id":"6E53726A-9A98-41CB-8A5D-D1138D298698"}
{
  "uptime" : 4000000,
  "procRole" : "Foreground",
  "version" : 2,
  "userID" : 501,
  "deployVersion" : 210,
  "modelCode" : "MacBookPro15,1",
  "coalitionID" : 657175,
  "osVersion" : {
    "train" : "macOS 15.0",
    "build" : "24A335",
    "releaseType" : "User"
  },
  "captureTime" : "2026-08-17 01:30:36.1195 -0600",
  "codeSigningMonitor" : 0,
  "incident" : "6E53726A-9A98-41CB-8A5D-D1138D298698",
  "pid" : 21869,
  "cpuType" : "X86-64",
  "roots_installed" : 0,
  "bug_type" : "309",
  "procLaunch" : "2026-08-17 01:30:34.3622 -0600",
  "procStartAbsTime" : 4071132281485309,
  "procExitAbsTime" : 4071133981984381,
  "procName" : "app",
  "procPath" : "\/Applications\/Nabu.app\/Contents\/MacOS\/app",
  "bundleInfo" : {"CFBundleShortVersionString":"0.1.0","CFBundleVersion":"0.1.0","CFBundleIdentifier":"md.nabu.app"},
  "storeInfo" : {"deviceIdentifierForVendor":"0F5522BC-0E5F-5F42-B77F-70AF81E8D7C8","thirdParty":true},
  "parentProc" : "launchd",
  "parentPid" : 1,
  "coalitionName" : "md.nabu.app",
  "crashReporterKey" : "155BD1B8-6489-F5E1-B1CC-D3DC2A035CCC",
  "lowPowerMode" : 1,
  "codeSigningID" : "",
  "codeSigningTeamID" : "",
  "codeSigningValidationCategory" : 0,
  "codeSigningTrustLevel" : 4294967295,
  "bootSessionUUID" : "1D737F45-A968-439D-8AB4-8CAA08BEBE0C",
  "wakeTime" : 18692,
  "bridgeVersion" : {"build":"22P353","train":"9.0"},
  "sleepWakeUUID" : "C291AF2C-6427-42B4-ADF4-E4241EF5FEB3",
  "sip" : "enabled",
  "exception" : {"codes":"0x0000000000000000, 0x0000000000000000","rawCodes":[0,0],"type":"EXC_CRASH","signal":"SIGABRT"},
  "termination" : {"flags":0,"code":6,"namespace":"SIGNAL","indicator":"Abort trap: 6","byProc":"app","byPid":21869},
  "ktriageinfo" : "VM - (arg = 0x3) mach_vm_allocate_kernel failed within call to vm_map_enter\nVM - (arg = 0x3) mach_vm_allocate_kernel failed within call to vm_map_enter\n",
  "asi" : {"libsystem_c.dylib":["abort() called"]},
  "extMods" : {"caller":{"thread_create":0,"thread_set_state":0,"task_for_pid":0},"system":{"thread_create":0,"thread_set_state":1786,"task_for_pid":121},"targeted":{"thread_create":0,"thread_set_state":0,"task_for_pid":0},"warnings":0},
  "faultingThread" : 0,
  "threads" : [{"threadState":{"r13":{"value":267264},"rax":{"value":0},"rflags":{"value":582},"cpu":{"value":0},"r14":{"value":259},"rsi":{"value":6},"r8":{"value":3},"cr2":{"value":0},"rdx":{"value":0},"r10":{"value":0},"r9":{"value":140281547668912},"r15":{"value":22},"rbx":{"value":6},"trap":{"value":133},"err":{"value":33554760},"r11":{"value":582},"rip":{"value":140703267490642,"matchesCrashFrame":1},"rbp":{"value":140701844037392},"rsp":{"value":140701844037352},"r12":{"value":105553117412544},"rcx":{"value":140701844037352},"flavor":"x86_THREAD_STATE","rdi":{"value":259}},"id":98098899,"triggered":true,"name":"main","queue":"com.apple.main-thread","frames":[{"imageOffset":31570,"symbol":"__pthread_kill","symbolLocation":10,"imageIndex":4},{"imageOffset":24453,"symbol":"pthread_kill","symbolLocation":262,"imageIndex":5},{"imageOffset":527129,"symbol":"abort","symbolLocation":126,"imageIndex":6},{"imageOffset":16383081,"symbol":"_RNvNtNtNtCsgejaSCmAoRz_3std3sys3pal4unix14abort_internal","symbolLocation":9,"imageIndex":0},{"imageOffset":16382521,"symbol":"_RNvNtCsgejaSCmAoRz_3std7process5abort","symbolLocation":9,"imageIndex":0},{"imageOffset":15815753,"symbol":"_RNvNtCsgejaSCmAoRz_3std9panicking15panic_with_hook","symbolLocation":892,"imageIndex":0},{"imageOffset":15709522,"symbol":"_RNCNvNtCsgejaSCmAoRz_3std9panicking13panic_handler0B5_","symbolLocation":114,"imageIndex":0},{"imageOffset":15661193,"symbol":"_RINvNtNtCsgejaSCmAoRz_3std3sys9backtrace26___rust_end_short_backtraceNCNvNtB6_9panicking13panic_handler0zEB6_","symbolLocation":9,"imageIndex":0},{"imageOffset":15712036,"symbol":"_RNvCs9wFQrvczXsK_7___rustc17rust_begin_unwind","symbolLocation":36,"imageIndex":0},{"imageOffset":16386732,"symbol":"_RNvNtCsbAqs9W1eE8G_4core9panicking18panic_nounwind_fmt","symbolLocation":44,"imageIndex":0},{"imageOffset":16386583,"symbol":"_RNvNtCsbAqs9W1eE8G_4core9panicking14panic_nounwind","symbolLocation":23,"imageIndex":0},{"imageOffset":16386994,"symbol":"_RNvNtCsbAqs9W1eE8G_4core9panicking19panic_cannot_unwind","symbolLocation":19,"imageIndex":0},{"imageOffset":5968189,"symbol":"_RNvNtNtNtCslaF18iYGa9m_3tao13platform_impl8platform12app_delegate20did_finish_launching","symbolLocation":301,"imageIndex":0},{"imageOffset":467052,"symbol":"__CFNOTIFICATIONCENTER_IS_CALLING_OUT_TO_AN_OBSERVER__","symbolLocation":137,"imageIndex":7},{"imageOffset":1047362,"symbol":"___CFXRegistrationPost_block_invoke","symbolLocation":88,"imageIndex":7},{"imageOffset":1047185,"symbol":"_CFXRegistrationPost","symbolLocation":530,"imageIndex":7},{"imageOffset":268204,"symbol":"_CFXNotificationPost","symbolLocation":765,"imageIndex":7},{"imageOffset":39851,"symbol":"-[NSNotificationCenter postNotificationName:object:userInfo:]","symbolLocation":82,"imageIndex":8},{"imageOffset":300981,"symbol":"-[NSApplication _postDidFinishNotification]","symbolLocation":311,"imageIndex":9},{"imageOffset":300282,"symbol":"-[NSApplication _sendFinishLaunchingNotification]","symbolLocation":215,"imageIndex":9},{"imageOffset":292000,"symbol":"-[NSApplication(NSAppleEventHandling) _handleAEOpenEvent:]","symbolLocation":542,"imageIndex":9},{"imageOffset":291059,"symbol":"-[NSApplication(NSAppleEventHandling) _handleCoreEvent:withReplyEvent:]","symbolLocation":679,"imageIndex":9},{"imageOffset":207889,"symbol":"-[NSAppleEventManager dispatchRawAppleEvent:withRawReply:handlerRefCon:]","symbolLocation":307,"imageIndex":8},{"imageOffset":207397,"symbol":"_NSAppleEventManagerGenericHandler","symbolLocation":80,"imageIndex":8},{"imageOffset":45957,"imageIndex":10},{"imageOffset":44051,"imageIndex":10},{"imageOffset":17688,"symbol":"aeProcessAppleEvent","symbolLocation":409,"imageIndex":10},{"imageOffset":160774,"symbol":"AEProcessAppleEvent","symbolLocation":55,"imageIndex":11},{"imageOffset":262338,"symbol":"_DPSNextEvent","symbolLocation":1725,"imageIndex":9},{"imageOffset":10863816,"symbol":"-[NSApplication(NSEventRouting) _nextEventMatchingEventMask:untilDate:inMode:dequeue:]","symbolLocation":1290,"imageIndex":9},{"imageOffset":200375,"symbol":"-[NSApplication run]","symbolLocation":610,"imageIndex":9},{"imageOffset":1244147,"symbol":"_RINvMs3_NtNtNtCslaF18iYGa9m_3tao13platform_impl8platform10event_loopINtB6_9EventLoopINtCsiQj4t0msQ3m_17tauri_runtime_wry7MessageNtCs3ALwlAwKxNk_5tauri16EventLoopMessageEE3runNCINvB1n_18make_event_handlerB22_NCINvMsf_NtB24_3appNtB3s_3App28make_run_event_loop_callbackNCNvCsk0WWsatWZTt_7app_lib3runs2_0E0E0EB4k_","symbolLocation":595,"imageIndex":0},{"imageOffset":1249354,"symbol":"_RINvMsf_NtCs3ALwlAwKxNk_5tauri3appNtB6_3App3runNCNvCsk0WWsatWZTt_7app_lib3runs2_0EBN_","symbolLocation":682,"imageIndex":0},{"imageOffset":3077177,"symbol":"_RNvCsk0WWsatWZTt_7app_lib3run","symbolLocation":1897,"imageIndex":0},{"imageOffset":6902,"symbol":"_RINvNtNtCsgejaSCmAoRz_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECsigLMue0smWE_3app","symbolLocation":6,"imageIndex":0},{"imageOffset":6924,"symbol":"_RNCINvNtCsgejaSCmAoRz_3std2rt10lang_startuE0CsigLMue0smWE_3app","symbolLocation":12,"imageIndex":0},{"imageOffset":15809755,"symbol":"_RNvNtCsgejaSCmAoRz_3std2rt19lang_start_internal","symbolLocation":875,"imageIndex":0},{"imageOffset":7004,"symbol":"main","symbolLocation":44,"imageIndex":0},{"imageOffset":25293,"symbol":"start","symbolLocation":1805,"imageIndex":12}]},{"id":98098910,"frames":[{"imageOffset":7116,"symbol":"start_wqthread","symbolLocation":0,"imageIndex":5}],"threadState":{"r13":{"value":0},"rax":{"value":33554800},"rflags":{"value":512},"cpu":{"value":0},"r14":{"value":1},"rsi":{"value":10499},"r8":{"value":409604},"cr2":{"value":0},"rdx":{"value":123145310654464},"r10":{"value":0},"r9":{"value":18446744073709551615},"r15":{"value":123145311177592},"rbx":{"value":123145311178752},"trap":{"value":133},"err":{"value":33554800},"r11":{"value":582},"rip":{"value":140703267711948},"rbp":{"value":0},"rsp":{"value":123145311178752},"r12":{"value":5193733},"rcx":{"value":0},"flavor":"x86_THREAD_STATE","rdi":{"value":123145311178752}}},{"id":98098911,"frames":[{"imageOffset":7116,"symbol":"start_wqthread","symbolLocation":0,"imageIndex":5}],"threadState":{"r13":{"value":0},"rax":{"value":33554800},"rflags":{"value":512},"cpu":{"value":0},"r14":{"value":1},"rsi":{"value":9475},"r8":{"value":409604},"cr2":{"value":0},"rdx":{"value":123145311191040},"r10":{"value":0},"r9":{"value":18446744073709551615},"r15":{"value":123145311714176},"rbx":{"value":123145311715328},"trap":{"value":133},"err":{"value":33554800},"r11":{"value":582},"rip":{"value":140703267711948},"rbp":{"value":0},"rsp":{"value":123145311715328},"r12":{"value":1982472},"rcx":{"value":0},"flavor":"x86_THREAD_STATE","rdi":{"value":123145311715328}}},{"id":98098930,"frames":[{"imageOffset":7116,"symbol":"start_wqthread","symbolLocation":0,"imageIndex":5}],"threadState":{"r13":{"value":0},"rax":{"value":33554800},"rflags":{"value":512},"cpu":{"value":0},"r14":{"value":0},"rsi":{"value":19971},"r8":{"value":409604},"cr2":{"value":0},"rdx":{"value":123145311727616},"r10":{"value":0},"r9":{"value":18446744073709551615},"r15":{"value":0},"rbx":{"value":123145312251904},"trap":{"value":133},"err":{"value":33554800},"r11":{"value":582},"rip":{"value":140703267711948},"rbp":{"value":0},"rsp":{"value":123145312251904},"r12":{"value":0},"rcx":{"value":0},"flavor":"x86_THREAD_STATE","rdi":{"value":123145312251904}}},{"id":98098932,"frames":[{"imageOffset":7116,"symbol":"start_wqthread","symbolLocation":0,"imageIndex":5}],"threadState":{"r13":{"value":0},"rax":{"value":33554800},"rflags":{"value":512},"cpu":{"value":0},"r14":{"value":1},"rsi":{"value":19211},"r8":{"value":409604},"cr2":{"value":0},"rdx":{"value":123145312264192},"r10":{"value":0},"r9":{"value":18446744073709551615},"r15":{"value":123145312787320},"rbx":{"value":123145312788480},"trap":{"value":133},"err":{"value":33554800},"r11":{"value":582},"rip":{"value":140703267711948},"rbp":{"value":0},"rsp":{"value":123145312788480},"r12":{"value":5193733},"rcx":{"value":0},"flavor":"x86_THREAD_STATE","rdi":{"value":123145312788480}}},{"id":98098933,"threadState":{"r13":{"value":123145313322832},"rax":{"value":18446744073709551612},"rflags":{"value":514},"cpu":{"value":0},"r14":{"value":16777217},"rsi":{"value":123145313322928},"r8":{"value":140704388575744,"symbolLocation":0,"symbol":"_dispatch_main_q"},"cr2":{"value":0},"rdx":{"value":0},"r10":{"value":0},"r9":{"value":3},"r15":{"value":0},"rbx":{"value":4294967295},"trap":{"value":133},"err":{"value":33554947},"r11":{"value":514},"rip":{"value":140703267468754},"rbp":{"value":123145313322736},"rsp":{"value":123145313322696},"r12":{"value":123145313322928},"rcx":{"value":123145313322696},"flavor":"x86_THREAD_STATE","rdi":{"value":16777217}},"queue":"com.apple.WebKit.ServicesController","frames":[{"imageOffset":9682,"symbol":"__ulock_wait","symbolLocation":10,"imageIndex":4},{"imageOffset":16378,"symbol":"_dlock_wait","symbolLocation":46,"imageIndex":14},{"imageOffset":16002,"symbol":"_dispatch_thread_event_wait_slow","symbolLocation":40,"imageIndex":14},{"imageOffset":68462,"symbol":"__DISPATCH_WAIT_FOR_QUEUE__","symbolLocation":307,"imageIndex":14},{"imageOffset":67482,"symbol":"_dispatch_sync_f_slow","symbolLocation":175,"imageIndex":14},{"imageOffset":6491464,"symbol":"void std::__1::__call_once_proxy[abi:sn180100]<std::__1::tuple<WebKit::ServicesController::refreshExistingServices(bool)::'block-literal'::$_5&&>>(void*)","symbolLocation":50,"imageIndex":15},{"imageOffset":45272,"symbol":"std::__1::__call_once(unsigned long volatile&, void*, void (*)(void*))","symbolLocation":146,"imageIndex":16},{"imageOffset":6443425,"symbol":"invocation function for block in WebKit::ServicesController::refreshExistingServices(bool)","symbolLocation":329,"imageIndex":15},{"imageOffset":9301,"symbol":"_dispatch_call_block_and_release","symbolLocation":12,"imageIndex":14},{"imageOffset":14306,"symbol":"_dispatch_client_callout","symbolLocation":8,"imageIndex":14},{"imageOffset":39259,"symbol":"_dispatch_lane_serial_drain","symbolLocation":739,"imageIndex":14},{"imageOffset":41954,"symbol":"_dispatch_lane_invoke","symbolLocation":377,"imageIndex":14},{"imageOffset":82139,"symbol":"_dispatch_root_queue_drain_deferred_wlh","symbolLocation":271,"imageIndex":14},{"imageOffset":80348,"symbol":"_dispatch_workloop_worker_thread","symbolLocation":659,"imageIndex":14},{"imageOffset":11391,"symbol":"_pthread_wqthread","symbolLocation":326,"imageIndex":5},{"imageOffset":7131,"symbol":"start_wqthread","symbolLocation":15,"imageIndex":5}]},{"id":98098934,"frames":[{"imageOffset":7116,"symbol":"start_wqthread","symbolLocation":0,"imageIndex":5}],"threadState":{"r13":{"value":0},"rax":{"value":33554800},"rflags":{"value":512},"cpu":{"value":0},"r14":{"value":1},"rsi":{"value":31495},"r8":{"value":409604},"cr2":{"value":0},"rdx":{"value":123145313337344},"r10":{"value":0},"r9":{"value":18446744073709551615},"r15":{"value":123145313860472},"rbx":{"value":123145313861632},"trap":{"value":133},"err":{"value":33554800},"r11":{"value":582},"rip":{"value":140703267711948},"rbp":{"value":0},"rsp":{"value":123145313861632},"r12":{"value":5128196},"rcx":{"value":0},"flavor":"x86_THREAD_STATE","rdi":{"value":123145313861632}}},{"id":98098946,"name":"JavaScriptCore libpas scavenger","threadState":{"r13":{"value":5501853107712},"rax":{"value":260},"rflags":{"value":583},"cpu":{"value":0},"r14":{"value":123145314398208},"rsi":{"value":5501853107712},"r8":{"value":0},"cr2":{"value":0},"rdx":{"value":0},"r10":{"value":0},"r9":{"value":160},"r15":{"value":0},"rbx":{"value":22},"trap":{"value":133},"err":{"value":33554737},"r11":{"value":582},"rip":{"value":140703267473834},"rbp":{"value":123145314398016},"rsp":{"value":123145314397864},"r12":{"value":124998944},"rcx":{"value":123145314397864},"flavor":"x86_THREAD_STATE","rdi":{"value":4760803456}},"frames":[{"imageOffset":14762,"symbol":"__psynch_cvwait","symbolLocation":10,"imageIndex":4},{"imageOffset":26536,"symbol":"_pthread_cond_wait","symbolLocation":1193,"imageIndex":5},{"imageOffset":27762599,"symbol":"scavenger_thread_main","symbolLocation":1799,"imageIndex":17},{"imageOffset":25171,"symbol":"_pthread_start","symbolLocation":99,"imageIndex":5},{"imageOffset":7151,"symbol":"thread_start","symbolLocation":15,"imageIndex":5}]},{"id":98098971,"name":"com.apple.coreanimation.render-server","threadState":{"r13":{"value":21525170190},"rax":{"value":268451845},"rflags":{"value":514},"cpu":{"value":0},"r14":{"value":2},"rsi":{"value":21525170190},"r8":{"value":0},"cr2":{"value":0},"rdx":{"value":8589934592},"r10":{"value":0},"r9":{"value":235308373245952},"r15":{"value":235308373245952},"rbx":{"value":123145315441344},"trap":{"value":133},"err":{"value":16777263},"r11":{"value":514},"rip":{"value":140703267462670},"rbp":{"value":123145315441184},"rsp":{"value":123145315441080},"r12":{"value":0},"rcx":{"value":123145315441080},"flavor":"x86_THREAD_STATE","rdi":{"value":123145315441344}},"frames":[{"imageOffset":3598,"symbol":"mach_msg2_trap","symbolLocation":10,"imageIndex":4},{"imageOffset":63010,"symbol":"mach_msg2_internal","symbolLocation":84,"imageIndex":4},{"imageOffset":32534,"symbol":"mach_msg_overwrite","symbolLocation":649,"imageIndex":4},{"imageOffset":4351,"symbol":"mach_msg","symbolLocation":19,"imageIndex":4},{"imageOffset":305073,"symbol":"CA::Render::Server::server_thread(void*)","symbolLocation":863,"imageIndex":18},{"imageOffset":304195,"symbol":"thread_fun(void*)","symbolLocation":25,"imageIndex":18},{"imageOffset":25171,"symbol":"_pthread_start","symbolLocation":99,"imageIndex":5},{"imageOffset":7151,"symbol":"thread_start","symbolLocation":15,"imageIndex":5}]},{"id":98098974,"name":"WebCore: Scrolling","threadState":{"r13":{"value":21592279046},"rax":{"value":268451845},"rflags":{"value":518},"cpu":{"value":0},"r14":{"value":2},"rsi":{"value":21592279046},"r8":{"value":0},"cr2":{"value":0},"rdx":{"value":8589934592},"r10":{"value":233109349990400},"r9":{"value":233109349990400},"r15":{"value":233109349990400},"rbx":{"value":123145315991488},"trap":{"value":133},"err":{"value":16777263},"r11":{"value":518},"rip":{"value":140703267462670},"rbp":{"value":123145315991328},"rsp":{"value":123145315991224},"r12":{"value":4294967295},"rcx":{"value":123145315991224},"flavor":"x86_THREAD_STATE","rdi":{"value":123145315991488}},"frames":[{"imageOffset":3598,"symbol":"mach_msg2_trap","symbolLocation":10,"imageIndex":4},{"imageOffset":63010,"symbol":"mach_msg2_internal","symbolLocation":84,"imageIndex":4},{"imageOffset":32534,"symbol":"mach_msg_overwrite","symbolLocation":649,"imageIndex":4},{"imageOffset":4351,"symbol":"mach_msg","symbolLocation":19,"imageIndex":4},{"imageOffset":511048,"symbol":"__CFRunLoopServiceMachPort","symbolLocation":143,"imageIndex":7},{"imageOffset":505549,"symbol":"__CFRunLoopRun","symbolLocation":1393,"imageIndex":7},{"imageOffset":502636,"symbol":"CFRunLoopRunSpecific","symbolLocation":536,"imageIndex":7},{"imageOffset":998932,"symbol":"CFRunLoopRun","symbolLocation":40,"imageIndex":7},{"imageOffset":2025666,"symbol":"WTF::Detail::CallableWrapper<WTF::RunLoop::create(WTF::ASCIILiteral, WTF::ThreadType, WTF::Thread::QOS)::$_0, void>::call()","symbolLocation":82,"imageIndex":17},{"imageOffset":2158365,"symbol":"WTF::Thread::entryPoint(WTF::Thread::NewThreadContext*)","symbolLocation":237,"imageIndex":17},{"imageOffset":12633,"symbol":"WTF::wtfThreadEntryPoint(void*)","symbolLocation":9,"imageIndex":17},{"imageOffset":25171,"symbol":"_pthread_start","symbolLocation":99,"imageIndex":5},{"imageOffset":7151,"symbol":"thread_start","symbolLocation":15,"imageIndex":5}]},{"id":98098978,"frames":[{"imageOffset":7116,"symbol":"start_wqthread","symbolLocation":0,"imageIndex":5}],"threadState":{"r13":{"value":0},"rax":{"value":33554800},"rflags":{"value":512},"cpu":{"value":0},"r14":{"value":1},"rsi":{"value":55559},"r8":{"value":409604},"cr2":{"value":0},"rdx":{"value":123145316544512},"r10":{"value":0},"r9":{"value":18446744073709551615},"r15":{"value":123145317067640},"rbx":{"value":123145317068800},"trap":{"value":133},"err":{"value":33554800},"r11":{"value":582},"rip":{"value":140703267711948},"rbp":{"value":0},"rsp":{"value":123145317068800},"r12":{"value":5193733},"rcx":{"value":0},"flavor":"x86_THREAD_STATE","rdi":{"value":123145317068800}}},{"id":98098991,"frames":[{"imageOffset":7116,"symbol":"start_wqthread","symbolLocation":0,"imageIndex":5}],"threadState":{"r13":{"value":0},"rax":{"value":33554800},"rflags":{"value":512},"cpu":{"value":0},"r14":{"value":0},"rsi":{"value":85507},"r8":{"value":409604},"cr2":{"value":0},"rdx":{"value":123145317081088},"r10":{"value":0},"r9":{"value":18446744073709551615},"r15":{"value":0},"rbx":{"value":123145317605376},"trap":{"value":133},"err":{"value":33554800},"r11":{"value":582},"rip":{"value":140703267711948},"rbp":{"value":0},"rsp":{"value":123145317605376},"r12":{"value":0},"rcx":{"value":0},"flavor":"x86_THREAD_STATE","rdi":{"value":123145317605376}}},{"id":98098994,"name":"CVDisplayLink","threadState":{"r13":{"value":27492085668352},"rax":{"value":260},"rflags":{"value":663},"cpu":{"value":0},"r14":{"value":6656},"rsi":{"value":27492085668352},"r8":{"value":0},"cr2":{"value":0},"rdx":{"value":0},"r10":{"value":0},"r9":{"value":65704},"r15":{"value":0},"rbx":{"value":22},"trap":{"value":133},"err":{"value":33554737},"r11":{"value":663},"rip":{"value":140703267473834},"rbp":{"value":123145318141408},"rsp":{"value":123145318141256},"r12":{"value":15747415},"rcx":{"value":123145318141256},"flavor":"x86_THREAD_STATE","rdi":{"value":140281575939192}},"frames":[{"imageOffset":14762,"symbol":"__psynch_cvwait","symbolLocation":10,"imageIndex":4},{"imageOffset":26585,"symbol":"_pthread_cond_wait","symbolLocation":1242,"imageIndex":5},{"imageOffset":15589,"symbol":"CVDisplayLink::waitUntil(unsigned long long)","symbolLocation":375,"imageIndex":19},{"imageOffset":11358,"symbol":"CVDisplayLink::runIOThread()","symbolLocation":526,"imageIndex":19},{"imageOffset":25171,"symbol":"_pthread_start","symbolLocation":99,"imageIndex":5},{"imageOffset":7151,"symbol":"thread_start","symbolLocation":15,"imageIndex":5}]}],
  "usedImages" : [
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 4504735744,
    "CFBundleShortVersionString" : "0.1.0",
    "CFBundleIdentifier" : "md.nabu.app",
    "size" : 23953408,
    "uuid" : "1b4fc5d6-6c48-393e-91ff-5b9bcac0ec00",
    "path" : "\/Applications\/Nabu.app\/Contents\/MacOS\/app",
    "name" : "app",
    "CFBundleVersion" : "0.1.0"
  },
  {
    "source" : "P",
    "arch" : "x86_64h",
    "base" : 4733095936,
    "size" : 53248,
    "uuid" : "a732c7f4-a3c1-39e5-9fc3-5e1deb73a584",
    "path" : "\/usr\/lib\/libobjc-trampolines.dylib",
    "name" : "libobjc-trampolines.dylib"
  },
  {
    "source" : "P",
    "arch" : "x86_64h",
    "base" : 4963463168,
    "CFBundleShortVersionString" : "6.1.13",
    "CFBundleIdentifier" : "com.apple.AMDRadeonX4000GLDriver",
    "size" : 1040384,
    "uuid" : "d3de2094-5363-3ff7-9a1e-3ec562f455c7",
    "path" : "\/System\/Library\/Extensions\/AMDRadeonX4000GLDriver.bundle\/Contents\/MacOS\/AMDRadeonX4000GLDriver",
    "name" : "AMDRadeonX4000GLDriver",
    "CFBundleVersion" : "6.0.1"
  },
  {
    "source" : "P",
    "arch" : "x86_64h",
    "base" : 5015187456,
    "CFBundleShortVersionString" : "7.0",
    "CFBundleIdentifier" : "com.apple.audio.codecs.Components",
    "size" : 10338304,
    "uuid" : "b93b4d2e-2550-3682-a891-fe1174c991fa",
    "path" : "\/System\/Library\/Components\/AudioCodecs.component\/Contents\/MacOS\/AudioCodecs",
    "name" : "AudioCodecs",
    "CFBundleVersion" : "7.0"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703267459072,
    "size" : 245760,
    "uuid" : "a0aee5ca-4298-3070-82f9-ea72229f36e5",
    "path" : "\/usr\/lib\/system\/libsystem_kernel.dylib",
    "name" : "libsystem_kernel.dylib"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703267704832,
    "size" : 49152,
    "uuid" : "c0db9cf9-86ec-31d4-a557-2c07945fd8f2",
    "path" : "\/usr\/lib\/system\/libsystem_pthread.dylib",
    "name" : "libsystem_pthread.dylib"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703266287616,
    "size" : 561144,
    "uuid" : "2d4e63ef-e31c-3cc1-94ec-2b7e28b9782f",
    "path" : "\/usr\/lib\/system\/libsystem_c.dylib",
    "name" : "libsystem_c.dylib"
  },
  {
    "source" : "P",
    "arch" : "x86_64h",
    "base" : 140703268159488,
    "CFBundleShortVersionString" : "6.9",
    "CFBundleIdentifier" : "com.apple.CoreFoundation",
    "size" : 4849651,
    "uuid" : "a7324227-eb88-3393-8efe-10a9f3d28064",
    "path" : "\/System\/Library\/Frameworks\/CoreFoundation.framework\/Versions\/A\/CoreFoundation",
    "name" : "CoreFoundation",
    "CFBundleVersion" : "3038.1.402"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703285219328,
    "CFBundleShortVersionString" : "6.9",
    "CFBundleIdentifier" : "com.apple.Foundation",
    "size" : 14917617,
    "uuid" : "b54a23dd-8603-361b-ad2e-54ac2cd8ac39",
    "path" : "\/System\/Library\/Frameworks\/Foundation.framework\/Versions\/C\/Foundation",
    "name" : "Foundation",
    "CFBundleVersion" : "3038.1.402"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703328382976,
    "CFBundleShortVersionString" : "6.9",
    "CFBundleIdentifier" : "com.apple.AppKit",
    "size" : 21606398,
    "uuid" : "55408426-52c7-3b83-9097-0a12aa2620e1",
    "path" : "\/System\/Library\/Frameworks\/AppKit.framework\/Versions\/C\/AppKit",
    "name" : "AppKit",
    "CFBundleVersion" : "2566"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703392157696,
    "CFBundleShortVersionString" : "944",
    "CFBundleIdentifier" : "com.apple.AE",
    "size" : 458752,
    "uuid" : "2b604f6e-cdc7-349b-80b5-6bf5faa9a9d2",
    "path" : "\/System\/Library\/Frameworks\/CoreServices.framework\/Versions\/A\/Frameworks\/AE.framework\/Versions\/A\/AE",
    "name" : "AE",
    "CFBundleVersion" : "944"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703459442688,
    "CFBundleShortVersionString" : "2.1.1",
    "CFBundleIdentifier" : "com.apple.HIToolbox",
    "size" : 2998261,
    "uuid" : "98a58f35-29b9-32ce-b1ba-5bde0bfe5ae2",
    "path" : "\/System\/Library\/Frameworks\/Carbon.framework\/Versions\/A\/Frameworks\/HIToolbox.framework\/Versions\/A\/HIToolbox",
    "name" : "HIToolbox"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703263977472,
    "size" : 574256,
    "uuid" : "e6056c94-fc2d-3517-b1e1-46d8eb58a10e",
    "path" : "\/usr\/lib\/dyld",
    "name" : "dyld"
  },
  {
    "size" : 0,
    "source" : "A",
    "base" : 0,
    "uuid" : "00000000-0000-0000-0000-000000000000"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703265980416,
    "size" : 294906,
    "uuid" : "6c0ff4e0-6f75-36fa-b45f-0075a398132d",
    "path" : "\/usr\/lib\/system\/libdispatch.dylib",
    "name" : "libdispatch.dylib"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140707687346176,
    "CFBundleShortVersionString" : "20619",
    "CFBundleIdentifier" : "com.apple.WebKit",
    "size" : 15486968,
    "uuid" : "6e1f3e30-5977-33d3-80cc-96385c75119c",
    "path" : "\/System\/Library\/Frameworks\/WebKit.framework\/Versions\/A\/WebKit",
    "name" : "WebKit",
    "CFBundleVersion" : "20619.1.26.31.6"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703266848768,
    "size" : 511996,
    "uuid" : "e35e82f9-4037-35da-99f0-4d09be1d9721",
    "path" : "\/usr\/lib\/libc++.1.dylib",
    "name" : "libc++.1.dylib"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140707558670336,
    "CFBundleShortVersionString" : "20619",
    "CFBundleIdentifier" : "com.apple.JavaScriptCore",
    "size" : 30109542,
    "uuid" : "d02e0fae-3c48-32fe-b010-ac8d1b8c0648",
    "path" : "\/System\/Library\/Frameworks\/JavaScriptCore.framework\/Versions\/A\/JavaScriptCore",
    "name" : "JavaScriptCore",
    "CFBundleVersion" : "20619.1.26.31.6"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703418007552,
    "CFBundleShortVersionString" : "1.11",
    "CFBundleIdentifier" : "com.apple.QuartzCore",
    "size" : 3788787,
    "uuid" : "681f04a8-f237-3d8a-a6e3-ea230e0678c5",
    "path" : "\/System\/Library\/Frameworks\/QuartzCore.framework\/Versions\/A\/QuartzCore",
    "name" : "QuartzCore",
    "CFBundleVersion" : "1149.6.2"
  },
  {
    "source" : "P",
    "arch" : "x86_64",
    "base" : 140703426387968,
    "CFBundleShortVersionString" : "1.8",
    "CFBundleIdentifier" : "com.apple.CoreVideo",
    "size" : 344052,
    "uuid" : "c07bd761-b2b4-351e-8793-f18b0ad28f2a",
    "path" : "\/System\/Library\/Frameworks\/CoreVideo.framework\/Versions\/A\/CoreVideo",
    "name" : "CoreVideo",
    "CFBundleVersion" : "648.29"
  }
],
  "sharedCache" : {
  "base" : 140703263227904,
  "size" : 25769803776,
  "uuid" : "78aaaa52-08a2-311d-a934-9c187f804833"
},
  "vmSummary" : "ReadOnly portion of Libraries: Total=1.2G resident=0K(0%) swapped_out_or_unallocated=1.2G(100%)\nWritable regions: Total=1.8G written=0K(0%) resident=0K(0%) swapped_out=0K(0%) unallocated=1.8G(100%)\n\n                                VIRTUAL   REGION \nREGION TYPE                        SIZE    COUNT (non-coalesced) \n===========                     =======  ======= \nActivity Tracing                   256K        1 \nColorSync                          244K       29 \nCoreAnimation                      240K       28 \nCoreGraphics                        16K        3 \nCoreServices                        60K        1 \nFoundation                          16K        1 \nIOKit                             15.5M        2 \nJS JIT generated code              1.0G        3 \nKernel Alloc Once                    8K        1 \nMALLOC                           658.4M       69 \nMALLOC guard page                   48K       12 \nSTACK GUARD                         48K       12 \nStack                             14.6M       13 \nStack Guard                       56.0M        1 \nVM_ALLOCATE                        208K       16 \nVM_ALLOCATE (reserved)             128K        1         reserved VM address space (unallocated)\nWebKit Malloc                    160.0M        4 \nWebKit Malloc (reserved)          32.0M        1         reserved VM address space (unallocated)\n__CTF                               824        1 \n__DATA                            30.7M      774 \n__DATA_CONST                      94.6M      789 \n__DATA_DIRTY                      1989K      277 \n__FONT_DATA                        2352        1 \n__GLSLBUILTINS                    5174K        1 \n__INFO_FILTER                         8        1 \n__LINKEDIT                       190.3M        5 \n__OBJC_RO                         76.1M        1 \n__OBJC_RW                         2354K        2 \n__TEXT                             1.0G      804 \n__TPRO_CONST                       272K        2 \nmapped file                      233.1M       25 \nowned unmapped memory              256K        1 \nshared memory                      792K       18 \n===========                     =======  ======= \nTOTAL                              3.5G     2900 \nTOTAL, minus reserved VM space     3.5G     2900 \n",
  "legacyInfo" : {
  "threadTriggered" : {
    "name" : "main",
    "queue" : "com.apple.main-thread"
  }
},
  "logWritingSignature" : "ef14efa4463d77d090fc427ab7780f4e4e9c6c48",
  "trialInfo" : {
  "rollouts" : [
    {
      "rolloutId" : "5fb4245a1bbfe8005e33a1e1",
      "factorPackIds" : {

      },
      "deploymentId" : 240000021
    },
    {
      "rolloutId" : "661464ecda55e5192b100804",
      "factorPackIds" : {

      },
      "deploymentId" : 240000005
    }
  ],
  "experiments" : [

  ]
}
}

Model: MacBookPro15,1, BootROM 2069.0.0.0.0 (iBridge: 22.16.10353.0.0,0), 6 processors, 6-Core Intel Core i9, 2.9 GHz, 16 GB, SMC 
Graphics: Intel UHD Graphics 630, Intel UHD Graphics 630, Built-In
Display: Color LCD, 2880 x 1800 Retina, Main, MirrorOff, Online
Graphics: Radeon Pro 560X, Radeon Pro 560X, PCIe, 4 GB
Memory Module: BANK 0/ChannelA-DIMM0, 8 GB, DDR4, 2400 MHz, Micron, 8ATF1G64HZ-2G6E1
Memory Module: BANK 2/ChannelB-DIMM0, 8 GB, DDR4, 2400 MHz, Micron, 8ATF1G64HZ-2G6E1
AirPort: spairport_wireless_card_type_wifi (0x14E4, 0x7BF), wl0: Jul 26 2024 22:09:35 version 9.30.514.0.32.5.94 FWID 01-47278712
AirPort: 
Bluetooth: Version (null), 0 services, 0 devices, 0 incoming serial ports
Network Service: Wi-Fi, AirPort, en0
USB Device: USB31Bus
USB Device: T2Bus
USB Device: Touch Bar Backlight
USB Device: Touch Bar Display
USB Device: Apple Internal Keyboard / Trackpad
USB Device: Headset
USB Device: Ambient Light Sensor
USB Device: FaceTime HD Camera (Built-in)
USB Device: Apple T2 Controller
Thunderbolt Bus: MacBook Pro, Apple Inc., 47.5
Thunderbolt Bus: MacBook Pro, Apple Inc., 47.5
